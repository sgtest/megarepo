import { ProxyMarked, transferHandlers, releaseProxy, TransferHandler, Remote } from 'comlink'
import { Subscription } from 'rxjs'
import { Subscribable, Unsubscribable } from 'sourcegraph'
import { hasProperty } from '../util/types'

/**
 * Tests whether a value is a WHATWG URL object.
 */
export const isURL = (value: unknown): value is URL =>
    typeof value === 'object' &&
    value !== null &&
    hasProperty('href')(value) &&
    hasProperty('toString')(value) &&
    typeof value.toString === 'function' &&
    // eslint-disable-next-line @typescript-eslint/no-base-to-string
    value.href === value.toString()

/**
 * Registers global comlink transfer handlers.
 * This needs to be called before using comlink.
 * Idempotent.
 */
export function registerComlinkTransferHandlers(): void {
    const urlTransferHandler: TransferHandler<URL, string> = {
        canHandle: isURL,
        serialize: url => [url.href, []],
        deserialize: urlString => new URL(urlString),
    }
    transferHandlers.set('URL', urlTransferHandler)
}

/**
 * Creates a synchronous Subscription that will unsubscribe the given proxied Subscription asynchronously.
 *
 * @param subscriptionPromise A Promise for a Subscription proxied from the other thread
 */
export const syncSubscription = (subscriptionPromise: Promise<Remote<Unsubscribable & ProxyMarked>>): Subscription =>
    // We cannot pass the proxy subscription directly to Rx because it is a Proxy that looks like a function
    // eslint-disable-next-line @typescript-eslint/no-misused-promises
    new Subscription(async function (this: any) {
        const subscriptionProxy = await subscriptionPromise
        await subscriptionProxy.unsubscribe()
        subscriptionProxy[releaseProxy]()

        this._unsubscribe = null // Workaround: rxjs doesn't null out the reference to this callback
        ;(subscriptionPromise as any) = null
    })

/**
 * Runs f and returns a resolved promise with its value or a rejected promise with its exception,
 * regardless of whether it returns a promise or not.
 */
export const tryCatchPromise = async <T>(f: () => T | Promise<T>): Promise<T> => f()

/**
 * Reports whether value is a Promise.
 */
export const isPromiseLike = (value: unknown): value is PromiseLike<unknown> =>
    typeof value === 'object' && value !== null && hasProperty('then')(value) && typeof value.then === 'function'

/**
 * Reports whether value is a {@link sourcegraph.Subscribable}.
 */
export const isSubscribable = (value: unknown): value is Subscribable<unknown> =>
    typeof value === 'object' &&
    value !== null &&
    hasProperty('subscribe')(value) &&
    typeof value.subscribe === 'function'

/**
 * Promisifies method calls and objects if specified, throws otherwise if there is no stub provided
 * NOTE: it does not handle ProxyMethods and callbacks yet
 * NOTE2: for testing purposes only!!
 */
export const pretendRemote = <T>(obj: Partial<T>): Remote<T> =>
    // eslint-disable-next-line @typescript-eslint/no-unsafe-return
    (new Proxy(obj, {
        get: (a, prop) => {
            if (prop in a) {
                if (typeof (a as any)[prop] !== 'function') {
                    return Promise.resolve((a as any)[prop])
                }

                return (...args: any[]) => Promise.resolve((a as any)[prop](...args))
            }
            throw new Error(`unspecified property in the stub ${prop.toString()}`)
        },
    }) as unknown) as Remote<T>
