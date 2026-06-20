#include "tt_fps.h"

#include <EGL/egl.h>
#include <dlfcn.h>
#include <string.h>

/* Real EGL functions -- resolved once on first call */
static EGLBoolean (*real_eglSwapBuffers)(EGLDisplay, EGLSurface) = NULL;
static void (*(*real_eglGetProcAddress)(const char *))(void) = NULL;

static void ensure_real_egl(void)
{
    if (real_eglSwapBuffers)
        return;

    /* Primary: RTLD_NEXT */
    real_eglSwapBuffers = (EGLBoolean (*)(EGLDisplay, EGLSurface))dlsym(RTLD_NEXT, "eglSwapBuffers");
    real_eglGetProcAddress = (void (*(*)(const char *))(void))dlsym(RTLD_NEXT, "eglGetProcAddress");

    /* Fallback: open libEGL directly */
    if (!real_eglSwapBuffers) {
        void *lib = dlopen("libEGL.so.1", RTLD_LAZY | RTLD_NOLOAD);
        if (!lib)
            lib = dlopen("libEGL.so.1", RTLD_LAZY);
        if (lib) {
            if (!real_eglSwapBuffers)
                real_eglSwapBuffers = (EGLBoolean (*)(EGLDisplay, EGLSurface))dlsym(lib, "eglSwapBuffers");
            if (!real_eglGetProcAddress)
                real_eglGetProcAddress = (void (*(*)(const char *))(void))dlsym(lib, "eglGetProcAddress");
        }
    }
}

/* Hook eglSwapBuffers */
__attribute__((__visibility__("default")))
EGLBoolean eglSwapBuffers(EGLDisplay dpy, EGLSurface surface)
{
    ensure_real_egl();

    int64_t now = tt_get_nano();
    tt_fps_on_present(now);

    if (real_eglSwapBuffers)
        return real_eglSwapBuffers(dpy, surface);
    return EGL_FALSE;
}

/* Hook eglGetProcAddress to intercept extension queries.
 * Some apps get their swap function pointer through this path. */
__attribute__((__visibility__("default")))
void (*eglGetProcAddress(const char *procname))(void)
{
    ensure_real_egl();

    /* If the app asks for eglSwapBuffers via eglGetProcAddress,
     * we return our hook. This covers apps that load EGL extensions
     * dynamically. */
    if (procname && strcmp(procname, "eglSwapBuffers") == 0)
        return (void (*)(void))eglSwapBuffers;  /* our hook */

    if (real_eglGetProcAddress)
        return real_eglGetProcAddress(procname);
    return NULL;
}
