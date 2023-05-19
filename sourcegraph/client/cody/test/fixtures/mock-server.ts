import express from 'express'

const SERVER_PORT = 49300

export const SERVER_URL = 'http://localhost:49300'
export const VALID_TOKEN = 'abcdefgh1234'

// Runs a stub Cody service for testing.
export async function run<T>(around: () => Promise<T>): Promise<T> {
    const app = express()
    app.use(express.json())

    app.post('/.api/completions/stream', (req, res) => {
        res.send('event: completion\ndata: {"completion": "hello from the assistant"}\n\nevent: done\ndata: {}\n\n')
    })

    app.post('/.api/graphql', (req, res) => {
        if (req.headers.authorization !== `token ${VALID_TOKEN}`) {
            res.sendStatus(401)
            return
        }

        const operation = new URL(req.url, 'https://example.com').search.replace(/^\?/, '')
        switch (operation) {
            case 'CurrentUser':
                res.send(JSON.stringify({ data: { currentUser: 'u' } }))
                break
            case 'IsContextRequiredForChatQuery':
                res.send(JSON.stringify({ data: { isContextRequiredForChatQuery: false } }))
                break
            default:
                res.sendStatus(400)
                break
        }
    })

    const server = app.listen(SERVER_PORT, () => {
        console.log(`Mock server listening on port ${SERVER_PORT}`)
    })

    const result = await around()
    server.close()

    return result
}
