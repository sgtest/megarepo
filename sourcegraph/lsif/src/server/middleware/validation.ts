import express from 'express'
import { query, ValidationChain, validationResult } from 'express-validator'
import { parseCursor } from '../pagination/cursor'

/**
 * Create a query string validator for a required non-empty string value.
 *
 * @param key The query string key.
 */
export const validateNonEmptyString = (key: string): ValidationChain =>
    query(key)
        .isString()
        .not()
        .isEmpty()

/**
 * Create a query string validator for a possibly empty string value.
 *
 * @param key The query string key.
 */
export const validateOptionalString = (key: string): ValidationChain =>
    query(key)
        .optional()
        .customSanitizer(value => value || '')

/**
 * Create a query string validator for a possibly empty boolean value.
 *
 * @param key The query string key.
 */
export const validateOptionalBoolean = (key: string): ValidationChain =>
    query(key)
        .optional()
        .isBoolean()
        .toBoolean()

/**
 * Create a query string validator for a possibly empty integer value.
 *
 * @param key The query string key.
 */
export const validateOptionalInt = (key: string): ValidationChain =>
    query(key)
        .optional()
        .isInt()
        .toInt()

/**
 * A validator used for a string query field.
 */
export const validateQuery = validateOptionalString('query')

/**
 * Create a validator for an integer limit field. Defaults to the given limit if not supplied.
 *
 * @param defaultValue The default value.
 */
export const validateLimit = (defaultValue: number): ValidationChain =>
    validateOptionalInt('limit').customSanitizer(value => value || defaultValue)

/**
 * A validator used for an integer offset field. Defaults to zero if not supplied.
 */
export const validateOffset = validateOptionalInt('offset').customSanitizer(value => value || 0)

/**
 * Create a validator for a cursor that is serialized as the supplied generic type.
 */
export const validateCursor = <T>(): ValidationChain =>
    validateOptionalString('cursor').customSanitizer(value => parseCursor<T>(value))

/**
 * Middleware function used to apply a sequence of validators and then return
 * an unprocessable entity response with an error message if validation fails.
 */
export const validationMiddleware = (chains: ValidationChain[]) => async (
    req: express.Request,
    res: express.Response,
    next: express.NextFunction
): Promise<void> => {
    await Promise.all(chains.map(chain => chain.run(req)))

    const errors = validationResult(req)
    if (!errors.isEmpty()) {
        res.status(422).send({ errors: errors.mapped() })
        return
    }

    next()
}
