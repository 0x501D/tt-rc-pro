#ifndef TT_FPS_H
#define TT_FPS_H

#include <stdint.h>
#include <pthread.h>

/* Default output file path */
#define TT_FPS_FILE_DEFAULT "/tmp/tt-rc-pro-fps"
#define TT_FPS_ENV_FILE     "TT_RC_PRO_FPS_FILE"
#define TT_FPS_ENV_ENABLE   "TT_RC_PRO_FPS"
#define TT_FPS_WINDOW_NS    (500LL * 1000000LL)  /* 500ms in nanoseconds */

/* Central FPS tracking state.
 * All hooks share one instance via tt_fps_global().
 * Protected by a mutex for thread safety.
 */
typedef struct {
    int       initialized;
    pthread_mutex_t lock;

    /* Timing */
    int64_t   last_present_ns;   /* CLOCK_MONOTONIC_RAW of last swap */
    int64_t   window_start_ns;   /* Start of current FPS window */
    int64_t   frame_count;       /* Frames in current window */

    /* Latest computed values */
    float     fps;
    float     frametime_ms;

    /* Output */
    char      filepath[512];
} tt_fps_state;

/* Get the global singleton state (lazy-initialized on first call). */
tt_fps_state *tt_fps_global(void);

/* Called by each hook on every swap/present.
 * Computes frametime, updates FPS window, writes file if window elapsed.
 * Pass the current time from clock_gettime(CLOCK_MONOTONIC_RAW).
 */
void tt_fps_on_present(int64_t now_ns);

/* Read the monotonic clock (CLOCK_MONOTONIC_RAW). */
int64_t tt_get_nano(void);

/* Write the fps/frametime line to the output file. */
void tt_fps_write_file(tt_fps_state *st);

#endif /* TT_FPS_H */
