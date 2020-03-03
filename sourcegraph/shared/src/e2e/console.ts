import { ConsoleMessage, ConsoleMessageType } from 'puppeteer'
import chalk, { Chalk } from 'chalk'
import * as util from 'util'
import terminalSize from 'term-size'
import stringWidth from 'string-width'
import { identity } from 'lodash'
import { asError } from '../util/errors'

const colors: Partial<Record<ConsoleMessageType, Chalk>> = {
    error: chalk.red,
    warning: chalk.yellow,
    info: chalk.cyan,
}
const icons: Partial<Record<ConsoleMessageType, string>> = {
    error: '✖',
    warning: '⚠',
    info: 'ℹ',
}

/**
 * Formats a console message that was logged in a Puppeteer Chrome instance for output on the NodeJS terminal.
 * Tries to mirror Chrome's console output as closely as possible and makes sense.
 */
export async function formatPuppeteerConsoleMessage(message: ConsoleMessage): Promise<string> {
    const color = colors[message.type()] ?? identity
    const icon = icons[message.type()] ?? ''
    const formattedLocation =
        'at ' +
        chalk.underline(
            [message.location().url, message.location().lineNumber, message.location().columnNumber]
                .filter(Boolean)
                .join(':')
        )
    // Right-align location, like in Chrome dev tools
    const locationLine = chalk.dim(
        formattedLocation &&
            '\n' +
                (!process.env.CI
                    ? ' '.repeat(terminalSize().columns - stringWidth(formattedLocation)) + formattedLocation
                    : '\t' + formattedLocation)
    )
    return [
        chalk.bold('🖥  Browser console:'),
        color(
            ...[
                message.type() !== 'log' ? chalk.bold(icon, message.type()) : '',
                message.args().length === 0 ? message.text() : '',
                ...(
                    await Promise.all(
                        message.args().map(async argHandle => {
                            try {
                                const json = await (
                                    await argHandle.evaluateHandle(value =>
                                        JSON.stringify(value, (key, value) => {
                                            // Check if value is error, because Errors are not serializable but commonly logged
                                            if (Object.prototype.toString.call(value) === '[object Error]') {
                                                return value.stack
                                            }
                                            return value
                                        })
                                    )
                                ).jsonValue()
                                return JSON.parse(json)
                            } catch (err) {
                                return chalk.italic(`[Could not serialize: ${asError(err).message}]`)
                            }
                        })
                    )
                ).map(value => (typeof value === 'string' ? value : util.inspect(value))),
                locationLine,
            ].filter(Boolean)
        ),
    ].join(' ')
}
