#include "tt_fps.h"

#include <GL/glx.h>
#include <dlfcn.h>

/* Real glXSwapBuffers -- resolved once on first call */
static void (*real_glXSwapBuffers)(Display *, GLXDrawable) = NULL;

static void ensure_real_glx_swap(void)
{
    if (real_glXSwapBuffers)
        return;

    /* RTLD_NEXT finds the next symbol in load order, skipping our override.
     * This is the standard LD_PRELOAD pattern and works without elfhacks. */
    real_glXSwapBuffers = (void (*)(Display *, GLXDrawable))dlsym(RTLD_NEXT, "glXSwapBuffers");

    if (!real_glXSwapBuffers) {
        /* Fallback: open libGL directly */
        void *lib = dlopen("libGL.so.1", RTLD_LAZY | RTLD_NOLOAD);
        if (!lib)
            lib = dlopen("libGL.so.1", RTLD_LAZY);
        if (lib)
            real_glXSwapBuffers = (void (*)(Display *, GLXDrawable))dlsym(lib, "glXSwapBuffers");
    }
}

/* Our hook -- exported with default visibility so LD_PRELOAD can find it */
__attribute__((__visibility__("default")))
void glXSwapBuffers(Display *dpy, GLXDrawable drawable)
{
    ensure_real_glx_swap();

    int64_t now = tt_get_nano();
    tt_fps_on_present(now);

    if (real_glXSwapBuffers)
        real_glXSwapBuffers(dpy, drawable);
}
