#include "tt_fps.h"

#include <vulkan/vulkan.h>
#include <vulkan/vk_layer.h>
#include <string.h>
#include <stdlib.h>

/* VK_LAYER_EXPORT may not be defined in all header versions */
#ifndef VK_LAYER_EXPORT
#define VK_LAYER_EXPORT __attribute__((__visibility__("default")))
#endif

/* ---- Dispatch table storage ---- */

typedef struct {
    PFN_vkGetInstanceProcAddr next_gipa;
    PFN_vkDestroyInstance     real_destroy_instance;
} InstanceData;

typedef struct {
    PFN_vkGetDeviceProcAddr   next_gdpa;
    PFN_vkDestroyDevice       real_destroy_device;
    PFN_vkQueuePresentKHR     real_queue_present;
} DeviceData;

typedef struct InstanceNode {
    VkInstance           instance;
    InstanceData         data;
    struct InstanceNode *next;
} InstanceNode;

typedef struct DeviceNode {
    VkDevice           device;
    DeviceData         data;
    struct DeviceNode *next;
} DeviceNode;

static InstanceNode *g_instances = NULL;
static DeviceNode   *g_devices   = NULL;
static pthread_mutex_t g_vulkan_lock = PTHREAD_MUTEX_INITIALIZER;

/* ---- Lookup helpers ---- */

static InstanceData *find_instance(VkInstance inst)
{
    for (InstanceNode *n = g_instances; n; n = n->next)
        if (n->instance == inst)
            return &n->data;
    return NULL;
}

static DeviceData *find_device(VkDevice dev)
{
    for (DeviceNode *n = g_devices; n; n = n->next)
        if (n->device == dev)
            return &n->data;
    return NULL;
}

/* ---- Interceptor: vkQueuePresentKHR ---- */

static VKAPI_ATTR VkResult VKAPI_CALL
hook_QueuePresentKHR(VkQueue queue, const VkPresentInfoKHR *pPresentInfo)
{
    /* Record frame timing BEFORE calling real present */
    int64_t now = tt_get_nano();
    tt_fps_on_present(now);

    /* Find any device with a real_queue_present set.
     * Typically there is only one device in a gaming scenario. */
    pthread_mutex_lock(&g_vulkan_lock);
    PFN_vkQueuePresentKHR present_fn = NULL;
    for (DeviceNode *n = g_devices; n; n = n->next) {
        if (n->data.real_queue_present) {
            present_fn = n->data.real_queue_present;
            break;
        }
    }
    pthread_mutex_unlock(&g_vulkan_lock);

    if (present_fn)
        return present_fn(queue, pPresentInfo);
    return VK_ERROR_DEVICE_LOST;
}

/* ---- Interceptor: vkCreateInstance ---- */

static VKAPI_ATTR VkResult VKAPI_CALL
hook_CreateInstance(const VkInstanceCreateInfo *pCreateInfo,
                    const VkAllocationCallbacks *pAllocator,
                    VkInstance *pInstance)
{
    /* Extract layer info from pCreateInfo chain - find VK_LAYER_LINK_INFO */
    VkLayerInstanceCreateInfo *layer_info = NULL;
    for (const VkBaseInStructure *chain = (const VkBaseInStructure *)pCreateInfo->pNext;
         chain; chain = chain->pNext) {
        if (chain->sType == VK_STRUCTURE_TYPE_LOADER_INSTANCE_CREATE_INFO) {
            VkLayerInstanceCreateInfo *candidate = (VkLayerInstanceCreateInfo *)chain;
            if (candidate->function == VK_LAYER_LINK_INFO) {
                layer_info = candidate;
                break;
            }
        }
    }

    if (!layer_info)
        return VK_ERROR_INITIALIZATION_FAILED;

    /* Get the next layer's GetInstanceProcAddr */
    PFN_vkGetInstanceProcAddr next_gipa =
        layer_info->u.pLayerInfo->pfnNextGetInstanceProcAddr;

    /* Advance the chain so the next layer sees correct info */
    layer_info->u.pLayerInfo = layer_info->u.pLayerInfo->pNext;

    /* Get the real vkCreateInstance */
    PFN_vkCreateInstance real_create =
        (PFN_vkCreateInstance)next_gipa(NULL, "vkCreateInstance");

    /* Call down the chain */
    VkResult result = real_create(pCreateInfo, pAllocator, pInstance);
    if (result != VK_SUCCESS)
        return result;

    /* Store instance data */
    InstanceNode *node = (InstanceNode *)calloc(1, sizeof(InstanceNode));
    if (!node)
        return VK_ERROR_OUT_OF_HOST_MEMORY;

    node->instance = *pInstance;
    node->data.next_gipa = next_gipa;
    node->data.real_destroy_instance =
        (PFN_vkDestroyInstance)next_gipa(*pInstance, "vkDestroyInstance");

    pthread_mutex_lock(&g_vulkan_lock);
    node->next = g_instances;
    g_instances = node;
    pthread_mutex_unlock(&g_vulkan_lock);

    return VK_SUCCESS;
}

/* ---- Interceptor: vkDestroyInstance ---- */

static VKAPI_ATTR void VKAPI_CALL
hook_DestroyInstance(VkInstance instance, const VkAllocationCallbacks *pAllocator)
{
    InstanceData *data = find_instance(instance);
    if (data && data->real_destroy_instance)
        data->real_destroy_instance(instance, pAllocator);

    /* Remove from list */
    pthread_mutex_lock(&g_vulkan_lock);
    InstanceNode **pp = &g_instances;
    while (*pp) {
        if ((*pp)->instance == instance) {
            InstanceNode *del = *pp;
            *pp = del->next;
            free(del);
            break;
        }
        pp = &(*pp)->next;
    }
    pthread_mutex_unlock(&g_vulkan_lock);
}

/* ---- Interceptor: vkCreateDevice ---- */

static VKAPI_ATTR VkResult VKAPI_CALL
hook_CreateDevice(VkPhysicalDevice physicalDevice,
                  const VkDeviceCreateInfo *pCreateInfo,
                  const VkAllocationCallbacks *pAllocator,
                  VkDevice *pDevice)
{
    /* Extract layer info - find VK_LAYER_LINK_INFO */
    VkLayerDeviceCreateInfo *layer_info = NULL;
    for (const VkBaseInStructure *chain = (const VkBaseInStructure *)pCreateInfo->pNext;
         chain; chain = chain->pNext) {
        if (chain->sType == VK_STRUCTURE_TYPE_LOADER_DEVICE_CREATE_INFO) {
            VkLayerDeviceCreateInfo *candidate = (VkLayerDeviceCreateInfo *)chain;
            if (candidate->function == VK_LAYER_LINK_INFO) {
                layer_info = candidate;
                break;
            }
        }
    }

    if (!layer_info)
        return VK_ERROR_INITIALIZATION_FAILED;

    PFN_vkGetInstanceProcAddr next_gipa =
        layer_info->u.pLayerInfo->pfnNextGetInstanceProcAddr;
    PFN_vkGetDeviceProcAddr next_gdpa =
        layer_info->u.pLayerInfo->pfnNextGetDeviceProcAddr;

    layer_info->u.pLayerInfo = layer_info->u.pLayerInfo->pNext;

    PFN_vkCreateDevice real_create =
        (PFN_vkCreateDevice)next_gipa(NULL, "vkCreateDevice");

    VkResult result = real_create(physicalDevice, pCreateInfo, pAllocator, pDevice);
    if (result != VK_SUCCESS)
        return result;

    /* Store device data */
    DeviceNode *node = (DeviceNode *)calloc(1, sizeof(DeviceNode));
    if (!node)
        return VK_ERROR_OUT_OF_HOST_MEMORY;

    node->device = *pDevice;
    node->data.next_gdpa = next_gdpa;
    node->data.real_destroy_device =
        (PFN_vkDestroyDevice)next_gdpa(*pDevice, "vkDestroyDevice");
    node->data.real_queue_present =
        (PFN_vkQueuePresentKHR)next_gdpa(*pDevice, "vkQueuePresentKHR");

    pthread_mutex_lock(&g_vulkan_lock);
    node->next = g_devices;
    g_devices = node;
    pthread_mutex_unlock(&g_vulkan_lock);

    return VK_SUCCESS;
}

/* ---- Interceptor: vkDestroyDevice ---- */

static VKAPI_ATTR void VKAPI_CALL
hook_DestroyDevice(VkDevice device, const VkAllocationCallbacks *pAllocator)
{
    DeviceData *data = find_device(device);
    if (data && data->real_destroy_device)
        data->real_destroy_device(device, pAllocator);

    pthread_mutex_lock(&g_vulkan_lock);
    DeviceNode **pp = &g_devices;
    while (*pp) {
        if ((*pp)->device == device) {
            DeviceNode *del = *pp;
            *pp = del->next;
            free(del);
            break;
        }
        pp = &(*pp)->next;
    }
    pthread_mutex_unlock(&g_vulkan_lock);
}

/* ---- Layer GetInstanceProcAddr ---- */

static VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL
layer_GetInstanceProcAddr(VkInstance instance, const char *pName)
{
    if (!pName)
        return NULL;

    if (strcmp(pName, "vkCreateInstance") == 0)
        return (PFN_vkVoidFunction)hook_CreateInstance;
    if (strcmp(pName, "vkDestroyInstance") == 0)
        return (PFN_vkVoidFunction)hook_DestroyInstance;
    if (strcmp(pName, "vkCreateDevice") == 0)
        return (PFN_vkVoidFunction)hook_CreateDevice;

    /* Forward to next layer */
    InstanceData *data = find_instance(instance);
    if (data && data->next_gipa)
        return data->next_gipa(instance, pName);
    return NULL;
}

/* ---- Layer GetDeviceProcAddr ---- */

static VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL
layer_GetDeviceProcAddr(VkDevice device, const char *pName)
{
    if (!pName)
        return NULL;

    if (strcmp(pName, "vkQueuePresentKHR") == 0)
        return (PFN_vkVoidFunction)hook_QueuePresentKHR;
    if (strcmp(pName, "vkDestroyDevice") == 0)
        return (PFN_vkVoidFunction)hook_DestroyDevice;

    /* Forward to next layer */
    DeviceData *data = find_device(device);
    if (data && data->next_gdpa)
        return data->next_gdpa(device, pName);
    return NULL;
}

/* ---- Layer interface negotiation (required entry point) ---- */

VK_LAYER_EXPORT VKAPI_ATTR VkResult VKAPI_CALL
vkNegotiateLoaderLayerInterfaceVersion(VkNegotiateLayerInterface *pVersionStruct)
{
    if (!pVersionStruct)
        return VK_ERROR_INITIALIZATION_FAILED;

    if (pVersionStruct->loaderLayerInterfaceVersion > CURRENT_LOADER_LAYER_INTERFACE_VERSION)
        pVersionStruct->loaderLayerInterfaceVersion = CURRENT_LOADER_LAYER_INTERFACE_VERSION;

    pVersionStruct->pfnGetInstanceProcAddr = layer_GetInstanceProcAddr;
    pVersionStruct->pfnGetDeviceProcAddr = layer_GetDeviceProcAddr;
    pVersionStruct->pfnGetPhysicalDeviceProcAddr = NULL;

    return VK_SUCCESS;
}
