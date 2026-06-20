#include "tt_fps.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

/* Global state */
static tt_fps_state g_state;
static pthread_once_t g_once = PTHREAD_ONCE_INIT;

static void tt_fps_init(tt_fps_state *st)
{
    memset(st, 0, sizeof(*st));
    pthread_mutex_init(&st->lock, NULL);

    /* Configurable output file path */
    const char *env = getenv(TT_FPS_ENV_FILE);
    if (env && env[0])
        snprintf(st->filepath, sizeof(st->filepath), "%s", env);
    else
        snprintf(st->filepath, sizeof(st->filepath), "%s", TT_FPS_FILE_DEFAULT);

    st->initialized = 1;
}

static void tt_fps_global_init(void)
{
    tt_fps_init(&g_state);
}

tt_fps_state *tt_fps_global(void)
{
    pthread_once(&g_once, tt_fps_global_init);
    return &g_state;
}

int64_t tt_get_nano(void)
{
    struct timespec tv;
    clock_gettime(CLOCK_MONOTONIC_RAW, &tv);
    return (int64_t)tv.tv_sec * 1000000000LL + (int64_t)tv.tv_nsec;
}

void tt_fps_write_file(tt_fps_state *st)
{
    /* Write atomically: write to a temp file, then rename.
     * This prevents the Rust reader from seeing a partial write.
     */
    char tmp_path[520];
    snprintf(tmp_path, sizeof(tmp_path), "%s.tmp", st->filepath);

    FILE *f = fopen(tmp_path, "w");
    if (!f)
        return;

    /* Format: "fps frametime_ms\n" -- matching what the Rust app reads */
    fprintf(f, "%.1f %.2f\n", st->fps, st->frametime_ms);
    fclose(f);

    rename(tmp_path, st->filepath);
}

void tt_fps_on_present(int64_t now_ns)
{
    tt_fps_state *st = tt_fps_global();
    pthread_mutex_lock(&st->lock);

    if (!st->initialized) {
        pthread_mutex_unlock(&st->lock);
        return;
    }

    /* First frame: just record the timestamp, no frametime */
    if (st->last_present_ns == 0) {
        st->last_present_ns = now_ns;
        st->window_start_ns = now_ns;
        st->frame_count = 1;
        pthread_mutex_unlock(&st->lock);
        return;
    }

    /* Frametime */
    int64_t frametime_ns = now_ns - st->last_present_ns;
    st->last_present_ns = now_ns;
    st->frametime_ms = (float)frametime_ns / 1000000.0f;

    /* Frame count for FPS window */
    st->frame_count++;

    /* Check if the FPS window has elapsed */
    int64_t elapsed_ns = now_ns - st->window_start_ns;
    if (elapsed_ns >= TT_FPS_WINDOW_NS) {
        /* FPS = frames / time */
        st->fps = (float)(1e9 * (double)st->frame_count / (double)elapsed_ns);

        /* Reset window */
        st->window_start_ns = now_ns;
        st->frame_count = 0;

        /* Write to file */
        tt_fps_write_file(st);
    }

    pthread_mutex_unlock(&st->lock);
}

void tt_fps_cleanup(void)
{
    tt_fps_state *st = tt_fps_global();
    pthread_mutex_lock(&st->lock);

    if (st->filepath[0]) {
        unlink(st->filepath);
        /* Also remove temp file if an atomic write was interrupted */
        char tmp_path[520];
        snprintf(tmp_path, sizeof(tmp_path), "%s.tmp", st->filepath);
        unlink(tmp_path);
    }

    st->initialized = 0;
    pthread_mutex_unlock(&st->lock);
}

/* Called when libttfps.so is unloaded (process exit / dlclose) */
__attribute__((destructor))
static void tt_fps_fini(void)
{
    tt_fps_cleanup();
}
