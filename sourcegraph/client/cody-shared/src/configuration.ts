export type ConfigurationUseContext = 'embeddings' | 'keyword' | 'none' | 'blended'

export interface Configuration {
    serverEndpoint: string
    codebase?: string
    debug: boolean
    useContext: ConfigurationUseContext
    experimentalSuggest: boolean
    experimentalChatPredictions: boolean
    experimentalInline: boolean
    experimentalGuardrails: boolean
    customHeaders: Record<string, string>
}

export interface ConfigurationWithAccessToken extends Configuration {
    /** The access token, which is stored in the secret storage (not configuration). */
    accessToken: string | null
}
