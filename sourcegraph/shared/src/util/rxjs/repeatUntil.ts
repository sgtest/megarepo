import { Observable } from 'rxjs'
import { repeatWhen, delay, takeWhile, repeat } from 'rxjs/operators'

/**
 * Mirrors values from the source observable and resubscribes to the source observable when it completes,
 * until an emission matches the provided condition. Resubscription is optionally delayed.
 */
export const repeatUntil = <T>(
    select: (value: T) => boolean,
    options?: {
        /**
         * The delay in milliseconds between resubscriptions.
         */
        delay: number
    }
) => (source: Observable<T>): Observable<T> =>
    source.pipe(
        options ? repeatWhen(completions => completions.pipe(delay(options.delay))) : repeat(),
        // Inclusive takeWhile so that the first value matching `select()` is emitted.
        takeWhile(value => !select(value), true)
    )
