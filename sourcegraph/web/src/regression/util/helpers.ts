import { GraphQLClient } from './GraphQLClient'
import { Driver } from '../../../../shared/src/e2e/driver'
import { gql, dataOrThrowErrors } from '../../../../shared/src/graphql/graphql'
import { catchError, map } from 'rxjs/operators'
import { throwError } from 'rxjs'
import { Key } from 'ts-key-enum'
import { deleteUser } from './api'
import { Config } from '../../../../shared/src/e2e/config'

/**
 * Create the user with the specified password
 */
export async function ensureLoggedInOrCreateTestUser(
    driver: Driver,
    gqlClient: GraphQLClient,
    {
        username,
        deleteIfExists,
        testUserPassword,
    }: {
        username: string
        deleteIfExists?: boolean
    } & Pick<Config, 'testUserPassword'>
): Promise<void> {
    if (!username.startsWith('test-')) {
        throw new Error(`Test username must start with "test-" (was ${JSON.stringify(username)})`)
    }

    if (deleteIfExists) {
        await deleteUser(gqlClient, username, false)
    } else {
        // Attempt to log in first
        try {
            await driver.ensureLoggedIn({ username, password: testUserPassword })
            return
        } catch (err) {
            console.log(`Login failed (error: ${err.message}), will attempt to create user ${JSON.stringify(username)}`)
        }
    }

    await createTestUser(driver, gqlClient, { username, testUserPassword })
    await driver.ensureLoggedIn({ username, password: testUserPassword })
}

async function createTestUser(
    driver: Driver,
    gqlClient: GraphQLClient,
    { username, testUserPassword }: { username: string } & Pick<Config, 'testUserPassword'>
): Promise<void> {
    // If there's an error, try to create the user
    const passwordResetURL = await gqlClient
        .mutateGraphQL(
            gql`
                mutation CreateUser($username: String!, $email: String) {
                    createUser(username: $username, email: $email) {
                        resetPasswordURL
                    }
                }
            `,
            { username }
        )
        .pipe(
            map(dataOrThrowErrors),
            catchError(err =>
                throwError(new Error(`Could not create user ${JSON.stringify(username)}: ${err.message})`))
            ),
            map(({ createUser }) => createUser.resetPasswordURL)
        )
        .toPromise()
    if (!passwordResetURL) {
        throw new Error('passwordResetURL was empty')
    }

    await driver.page.goto(passwordResetURL)
    await driver.page.keyboard.type(testUserPassword)
    await driver.page.keyboard.down(Key.Enter)

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    await driver.page.waitForFunction(() => document.body.textContent!.includes('Your password was reset'))
}
