import { ClientCodeEditorAPI } from './codeEditor'
import { ClientCommandsAPI } from './commands'
import { ClientConfigurationAPI } from './configuration'
import { ClientContextAPI } from './context'
import { ClientLanguageFeaturesAPI } from './languageFeatures'
import { ClientSearchAPI } from './search'
import { ClientViewsAPI } from './views'
import { ClientWindowsAPI } from './windows'

/**
 * The API that is exposed from the client (main thread) to the extension host (worker)
 */
export interface ClientAPI {
    ping(): 'pong'

    context: ClientContextAPI
    configuration: ClientConfigurationAPI
    search: ClientSearchAPI
    languageFeatures: ClientLanguageFeaturesAPI
    commands: ClientCommandsAPI
    windows: ClientWindowsAPI
    codeEditor: ClientCodeEditorAPI
    views: ClientViewsAPI
}
