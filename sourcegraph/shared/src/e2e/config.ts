import { pick } from 'lodash'

/**
 * Defines configuration for e2e tests. This is as-yet incomplete as some config
 * depended on by other modules is not included here.
 */
export interface Config {
    sudoToken: string
    sudoUsername: string
    gitHubClientID: string
    gitHubClientSecret: string
    gitHubToken: string
    gitHubUserBobPassword: string
    gitHubUserAmyPassword: string
    sourcegraphBaseUrl: string
    managementConsoleUrl: string
    includeAdminOnboarding: boolean
    testUserPassword: string
    noCleanup: boolean
    logBrowserConsole: boolean
    slowMo: number
    headless: boolean
    keepBrowser: boolean
}

interface Field<T = string> {
    envVar: string
    description?: string
    defaultValue?: T
}

interface FieldParser<T = string> {
    parser: (rawValue: string) => T
}

type ConfigFields = {
    [K in keyof Config]: Field<Config[K]> & (Config[K] extends string ? Partial<FieldParser> : FieldParser<Config[K]>)
}

const parseBool = (s: string): boolean => {
    if (['1', 't', 'true'].some(v => v.toLowerCase() === s)) {
        return true
    }
    if (['0', 'f', 'false'].some(v => v.toLowerCase() === s)) {
        return false
    }
    throw new Error(`could not parse string ${JSON.stringify(s)} to boolean`)
}

const configFields: ConfigFields = {
    sudoToken: {
        envVar: 'SOURCEGRAPH_SUDO_TOKEN',
        description:
            'An access token with "site-admin:sudo" permissions. This will be used to impersonate users in requests.',
    },
    sudoUsername: {
        envVar: 'SOURCEGRAPH_SUDO_USER',
        description: 'The site-admin-level username that will be impersonated with the sudo access token.',
    },
    gitHubClientID: {
        envVar: 'GITHUB_CLIENT_ID',
        description: 'Client ID of the GitHub app to use to authenticate to Sourcegraph.',
        defaultValue: 'cf9491b706c4c3b1f956', // "Local dev sign-in via GitHub" OAuth app from github.com/sourcegraph
    },
    gitHubClientSecret: {
        envVar: 'GITHUB_CLIENT_SECRET',
        description: 'Cilent secret of the GitHub app to use to authenticate to Sourcegraph.',
    },
    gitHubToken: {
        envVar: 'GITHUB_TOKEN',
        description:
            'A GitHub personal access token that will be used to authenticate a GitHub external service. It does not need to have any scopes.',
    },
    gitHubUserBobPassword: {
        envVar: 'GITHUB_USER_BOB_PASSWORD',
        description: 'Password of the GitHub user sg-e2e-regression-test-bob, used to log into Sourcegraph.',
    },
    gitHubUserAmyPassword: {
        envVar: 'GITHUB_USER_AMY_PASSWORD',
        description: 'Password of the GitHub user sg-e2e-regression-test-amy, used to log into Sourcegraph.',
    },
    sourcegraphBaseUrl: {
        envVar: 'SOURCEGRAPH_BASE_URL',
        defaultValue: 'http://localhost:3080',
        description:
            'The base URL of the Sourcegraph instance, e.g., https://sourcegraph.sgdev.org or http://localhost:3080.',
    },
    managementConsoleUrl: {
        envVar: 'MANAGEMENT_CONSOLE_URL',
        defaultValue: 'https://localhost:2633',
        description: 'URL at which the management console is accessible.',
    },
    includeAdminOnboarding: {
        envVar: 'INCLUDE_ADMIN_ONBOARDING',
        parser: parseBool,
        description:
            'If true, include admin onboarding tests, which assume none of the admin onboarding steps have yet completed on the instance. If those steps have already been completed, this test will fail.',
    },
    testUserPassword: {
        envVar: 'TEST_USER_PASSWORD',
        description:
            'The password to use for any test users that are created. This password should be secure and unguessable when running against production Sourcegraph instances.',
    },
    noCleanup: {
        envVar: 'NO_CLEANUP',
        parser: parseBool,
        description:
            "If true, regression tests will not clean up users, external services, or other resources they create. Set this to true if running against a dev instance (as it'll make test runs faster). Set to false if running against production",
    },
    keepBrowser: {
        envVar: 'KEEP_BROWSER',
        parser: parseBool,
        description: 'If true, browser window will remain open after tests run',
        defaultValue: false,
    },
    logBrowserConsole: {
        envVar: 'LOG_BROWSER_CONSOLE',
        parser: parseBool,
        description: 'If true, log browser console to stdout',
        defaultValue: false,
    },
    slowMo: {
        envVar: 'SLOWMO',
        parser: parseInt,
        description: 'Slow down puppeteer by the specified number of milliseconds',
        defaultValue: 0,
    },
    headless: {
        envVar: 'HEADLESS',
        parser: parseBool,
        description: 'Run Puppeteer in headless mode',
        defaultValue: false,
    },
}

/**
 * Reads e2e config from environment variables. The caller should specify the config fields that it
 * depends on. This should NOT be called from helper packages. Instead, call it near the start of
 * "test main" function (i.e., Jest `test` blocks). Doing this ensures that all the necessary
 * environment variables necessary for a test are presented to the user in one go.
 */
export function getConfig<T extends keyof Config>(...required: T[]): Pick<Config, T> {
    // Read config
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const config: { [key: string]: any } = {}
    for (const fieldName of required) {
        const field = configFields[fieldName]
        if (field.defaultValue !== undefined) {
            config[fieldName] = field.defaultValue
        }
        const envValue = process.env[field.envVar]
        if (envValue) {
            config[fieldName] = field.parser ? field.parser(envValue) : envValue
        }
    }

    // Check required fields for type safety
    const missingKeys = required.filter(key => config[key] === undefined)
    if (missingKeys.length > 0) {
        const fieldInfo = (k: T): string => {
            const field = configFields[k]
            if (!field) {
                return ''
            }
            const info = [field.envVar]
            if (field.defaultValue) {
                info.push(`default value: ${field.defaultValue}`)
            }
            if (field.description) {
                info.push(`description: ${field.description}`)
            }
            return `${info.join(', ')}`
        }
        throw new Error(`FAIL: Required config was not provided. These environment variables were missing:

${missingKeys.map(k => `- ${fieldInfo(k)}`).join('\n')}

The recommended way to set them is to install direnv (https://direnv.net) and
create a .envrc file at the root of this repository.
        `)
    }

    return pick(config, required)
}
