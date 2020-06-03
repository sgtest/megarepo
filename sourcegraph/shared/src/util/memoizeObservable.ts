import { Observable } from 'rxjs'
import { publishReplay, refCount, tap } from 'rxjs/operators'

let allCachesResetSeq = 0

/**
 * Clears all memoized data for memoizeObservable calls. All calls made to those functions after
 * clearing will result in the fetch func being called again.
 *
 * You must call this function after you've modified a memoized resource, or else some components of
 * the UI may have a stale view of the resource.
 */
export function resetAllMemoizationCaches(): void {
    allCachesResetSeq++
}

/**
 * Creates a function that memoizes the observable result of func.
 * If the Observable errors, the value will not be cached.
 *
 * @param resolver Determines the cache key for storing the result based on
 * the first argument provided to the memoized function.
 */
export function memoizeObservable<P, T>(
    func: (params: P) => Observable<T>,
    resolver: (params: P) => string
): (params: P, force?: boolean) => Observable<T> {
    const cache = new Map<string, Observable<T>>()
    let cacheResetSeq = allCachesResetSeq
    return (parameters: P, force = false) => {
        // Reset cache if resetAllMemoizationCaches was called.
        if (cacheResetSeq < allCachesResetSeq) {
            cache.clear()
            cacheResetSeq = allCachesResetSeq
        }

        const key = resolver(parameters)
        const hit = cache.get(key)
        if (!force && hit) {
            return hit
        }
        const observable = func(parameters).pipe(
            publishReplay(),
            refCount(),
            tap({
                error: () => {
                    cache.delete(key)
                },
            })
        )
        cache.set(key, observable)
        return observable
    }
}
