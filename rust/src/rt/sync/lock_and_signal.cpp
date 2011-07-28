#include "../globals.h"

/*
 * A "lock-and-signal" pair. These are necessarily coupled on pthreads
 * systems, and artificially coupled (by this file) on win32. Put
 * together here to minimize ifdefs elsewhere; you must use them as
 * if you're using a pthreads cvar+mutex pair.
 */

#include "lock_and_signal.h"

#if defined(__WIN32__)
lock_and_signal::lock_and_signal() 
    : alive(true)
{
    // FIXME: In order to match the behavior of pthread_cond_broadcast on
    // Windows, we create manual reset events. This however breaks the
    // behavior of pthread_cond_signal, fixing this is quite involved:
    // refer to: http://www.cs.wustl.edu/~schmidt/win32-cv-1.html

    _event = CreateEvent(NULL, TRUE, FALSE, NULL);
    InitializeCriticalSection(&_cs);
}

#else
lock_and_signal::lock_and_signal()
    : _locked(false), alive(true)
{
    CHECKED(pthread_cond_init(&_cond, NULL));
    CHECKED(pthread_mutex_init(&_mutex, NULL));
}
#endif

lock_and_signal::~lock_and_signal() {
#if defined(__WIN32__)
    CloseHandle(_event);
#else
    CHECKED(pthread_cond_destroy(&_cond));
    CHECKED(pthread_mutex_destroy(&_mutex));
#endif
    alive = false;
}

void lock_and_signal::lock() {
#if defined(__WIN32__)
    EnterCriticalSection(&_cs);
    _holding_thread = GetCurrentThreadId();
#else
    CHECKED(pthread_mutex_lock(&_mutex));
    _holding_thread = pthread_self();
#endif
    _locked = true;
}

void lock_and_signal::unlock() {
    _locked = false;
#if defined(__WIN32__)
    LeaveCriticalSection(&_cs);
#else
    CHECKED(pthread_mutex_unlock(&_mutex));
#endif
}

/**
 * Wait indefinitely until condition is signaled.
 */
void lock_and_signal::wait() {
    timed_wait(0);
}

bool lock_and_signal::timed_wait(size_t timeout_in_ns) {
    _locked = false;
    bool rv = true;
#if defined(__WIN32__)
    LeaveCriticalSection(&_cs);
    WaitForSingleObject(_event, INFINITE);
    EnterCriticalSection(&_cs);
    _holding_thread = GetCurrentThreadId();
#else
    if (timeout_in_ns == 0) {
        CHECKED(pthread_cond_wait(&_cond, &_mutex));
    } else {
        timeval time_val;
        gettimeofday(&time_val, NULL);
        timespec time_spec;
        time_spec.tv_sec = time_val.tv_sec + 0;
        time_spec.tv_nsec = time_val.tv_usec * 1000 + timeout_in_ns;
        if(time_spec.tv_nsec >= 1000000000) {
            time_spec.tv_sec++;
            time_spec.tv_nsec -= 1000000000;
        }
        int cond_wait_status
            = pthread_cond_timedwait(&_cond, &_mutex, &time_spec);
        switch(cond_wait_status) {
        case 0:
            // successfully grabbed the lock.
            break;
        case ETIMEDOUT:
            // Oops, we timed out.
            rv = false;
            break;
        default:
            // Error
            CHECKED(cond_wait_status);
        }
    }
    _holding_thread = pthread_self();
#endif
    _locked = true;
    return rv;
}

/**
 * Signal condition, and resume the waiting thread.
 */
void lock_and_signal::signal() {
#if defined(__WIN32__)
    SetEvent(_event);
#else
    CHECKED(pthread_cond_signal(&_cond));
#endif
}

/**
 * Signal condition, and resume all waiting threads.
 */
void lock_and_signal::signal_all() {
#if defined(__WIN32__)
    SetEvent(_event);
#else
    CHECKED(pthread_cond_broadcast(&_cond));
#endif
}

bool lock_and_signal::lock_held_by_current_thread()
{
#if defined(__WIN32__)
    return _locked && _holding_thread == GetCurrentThreadId();
#else
    return _locked && _holding_thread == pthread_self();
#endif
}

scoped_lock::scoped_lock(lock_and_signal &lock)
    : lock(lock)
{
    lock.lock();
}

scoped_lock::~scoped_lock()
{
    lock.unlock();
}

//
// Local Variables:
// mode: C++
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
