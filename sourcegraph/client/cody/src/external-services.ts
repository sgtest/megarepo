import { ChatClient } from '@sourcegraph/cody-shared/src/chat/chat'
import { CodebaseContext } from '@sourcegraph/cody-shared/src/codebase-context'
import { Editor } from '@sourcegraph/cody-shared/src/editor'
import { SourcegraphEmbeddingsSearchClient } from '@sourcegraph/cody-shared/src/embeddings/client'
import { IntentDetector } from '@sourcegraph/cody-shared/src/intent-detector'
import { SourcegraphIntentDetectorClient } from '@sourcegraph/cody-shared/src/intent-detector/client'
import { SourcegraphCompletionsClient } from '@sourcegraph/cody-shared/src/sourcegraph-api/completions'
import { SourcegraphGraphQLAPIClient } from '@sourcegraph/cody-shared/src/sourcegraph-api/graphql'
import { isError } from '@sourcegraph/cody-shared/src/utils'

import { getAccessToken, SecretStorage } from './command/secret-storage'
import { LocalKeywordContextFetcher } from './keyword-context/local-keyword-context-fetcher'

interface ExternalServices {
    intentDetector: IntentDetector
    codebaseContext: CodebaseContext
    chatClient: ChatClient
}

export async function configureExternalServices(
    serverEndpoint: string,
    codebase: string,
    rgPath: string,
    editor: Editor,
    secretStorage: SecretStorage,
    contextType: 'embeddings' | 'keyword' | 'none' | 'blended',
    mode: 'development' | 'production'
): Promise<ExternalServices> {
    const accessToken = await getAccessToken(secretStorage)
    const client = new SourcegraphGraphQLAPIClient(serverEndpoint, accessToken)
    const completions = new SourcegraphCompletionsClient(serverEndpoint, accessToken, mode)

    const repoId = codebase ? await client.getRepoId(codebase) : null
    if (isError(repoId)) {
        const errorMessage =
            `Cody could not find the '${codebase}' repository on your Sourcegraph instance.\n` +
            'Please check that the repository exists and is entered correctly in the cody.codebase setting.'
        console.error(errorMessage)
    }
    const embeddingsSearch = repoId && !isError(repoId) ? new SourcegraphEmbeddingsSearchClient(client, repoId) : null

    const codebaseContext = new CodebaseContext(
        contextType,
        embeddingsSearch,
        new LocalKeywordContextFetcher(rgPath, editor)
    )

    return {
        intentDetector: new SourcegraphIntentDetectorClient(client),
        codebaseContext,
        chatClient: new ChatClient(completions),
    }
}
