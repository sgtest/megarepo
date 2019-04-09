import { PlatformContext } from '../../platform/context'
import { ReferenceParams } from '../protocol'
import { createContextService } from './context/contextService'
import { CommandRegistry } from './services/command'
import { ContributionRegistry } from './services/contribution'
import { TextDocumentDecorationProviderRegistry } from './services/decoration'
import { ExtensionsService } from './services/extensionsService'
import { TextDocumentHoverProviderRegistry } from './services/hover'
import { TextDocumentLocationProviderIDRegistry, TextDocumentLocationProviderRegistry } from './services/location'
import { createModelService } from './services/modelService'
import { NotificationsService } from './services/notifications'
import { QueryTransformerRegistry } from './services/queryTransformer'
import { createSettingsService } from './services/settings'
import { ViewProviderRegistry } from './services/view'
import { createWorkspaceService } from './services/workspaceService'

/**
 * Services is a container for all services used by the client application.
 */
export class Services {
    constructor(
        private platformContext: Pick<
            PlatformContext,
            | 'settings'
            | 'updateSettings'
            | 'queryGraphQL'
            | 'getScriptURLForExtension'
            | 'clientApplication'
            | 'sideloadedExtensionURL'
        >
    ) {}

    public readonly commands = new CommandRegistry()
    public readonly context = createContextService(this.platformContext)
    public readonly model = createModelService()
    public readonly workspace = createWorkspaceService()
    public readonly notifications = new NotificationsService()
    public readonly settings = createSettingsService(this.platformContext)
    public readonly contribution = new ContributionRegistry(this.model.model, this.settings, this.context.data)
    public readonly extensions = new ExtensionsService(this.platformContext, this.model.model, this.settings)
    public readonly textDocumentDefinition = new TextDocumentLocationProviderRegistry()
    public readonly textDocumentReferences = new TextDocumentLocationProviderRegistry<ReferenceParams>()
    public readonly textDocumentLocations = new TextDocumentLocationProviderIDRegistry()
    public readonly textDocumentHover = new TextDocumentHoverProviderRegistry()
    public readonly textDocumentDecoration = new TextDocumentDecorationProviderRegistry()
    public readonly queryTransformer = new QueryTransformerRegistry()
    public readonly views = new ViewProviderRegistry()
}
