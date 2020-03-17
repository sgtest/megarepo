import { readEnvInt } from '../shared/settings'

/** Which port to run the worker metrics server on. Defaults to 3187. */
export const WORKER_METRICS_PORT = readEnvInt('WORKER_METRICS_PORT', 3187)

/** The interval (in seconds) to poll the database for unconverted uploads. */
export const WORKER_POLLING_INTERVAL = readEnvInt('WORKER_POLLING_INTERVAL', 1)

/** Where on the file system to store LSIF files. */
export const STORAGE_ROOT = process.env.LSIF_STORAGE_ROOT || 'lsif-storage'

/**
 * The target results per result chunk. This is used to determine the number of chunks
 * created during conversion, but does not guarantee that the distribution of hash keys
 * will wbe even. In practice, chunks are fairly evenly filled.
 */
export const RESULTS_PER_RESULT_CHUNK = readEnvInt('RESULTS_PER_RESULT_CHUNK', 500)

/** The maximum number of result chunks that will be created during conversion. */
export const MAX_NUM_RESULT_CHUNKS = readEnvInt('MAX_NUM_RESULT_CHUNKS', 1000)
