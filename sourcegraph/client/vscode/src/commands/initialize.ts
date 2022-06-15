import vscode from 'vscode'

import { EventSource } from '@sourcegraph/shared/src/graphql-operations'

import { version } from '../../package.json'
import { logEvent } from '../backend/eventLogger'
import { VSCE_COMMANDS_PARAMS } from '../common/links'
import { SourcegraphUri } from '../file-system/SourcegraphUri'
import { LocalStorageService, ANONYMOUS_USER_ID_KEY } from '../settings/LocalStorageService'

import { browserActions } from './browserActionsNode'

export function initializeCodeSharingCommands(
    context: vscode.ExtensionContext,
    eventSourceType: EventSource,
    localStorageService: LocalStorageService
): void {
    // Open local file or remote Sourcegraph file in browser
    context.subscriptions.push(
        vscode.commands.registerCommand('sourcegraph.openInBrowser', async () => {
            await browserActions('open', logRedirectEvent)
        })
    )
    // Copy Sourcegraph link to file
    context.subscriptions.push(
        vscode.commands.registerCommand('sourcegraph.copyFileLink', async () => {
            await browserActions('copy', logRedirectEvent)
        })
    )
    // Search Selected Text in Sourcegraph Search Tab
    context.subscriptions.push(
        vscode.commands.registerCommand('sourcegraph.selectionSearchWeb', async () => {
            const instanceUrl =
                vscode.workspace.getConfiguration('sourcegraph').get<string>('url') || 'https://sourcegraph.com'
            const editor = vscode.window.activeTextEditor
            const selectedQuery = editor?.document.getText(editor.selection)
            if (!editor || !selectedQuery) {
                throw new Error('No selection detected')
            }
            const uri = `${instanceUrl}/search?q=context:global+${encodeURIComponent(
                selectedQuery
            )}&patternType=literal${vsceUtms}`
            await vscode.env.openExternal(vscode.Uri.parse(uri))
        })
    )
    // Log Redirect Event
    function logRedirectEvent(sourcegraphUrl: string): void {
        const userEventVariables = {
            event: 'IDERedirected',
            userCookieID: localStorageService.getValue(ANONYMOUS_USER_ID_KEY),
            referrer: 'VSCE',
            url: sourcegraphUrl,
            source: eventSourceType,
            argument: JSON.stringify({ editor: 'vscode', version }),
        }
        logEvent(userEventVariables)
    }
}

export const vsceUtms = '&' + VSCE_COMMANDS_PARAMS

export function generateSourcegraphBlobLink(
    uri: vscode.Uri,
    startLine: number,
    startChar: number,
    endLine: number,
    endChar: number
): string {
    const instanceUrl = new URL(
        vscode.workspace.getConfiguration('sourcegraph').get<string>('url') || 'https://sourcegraph.com'
    )
    // Using SourcegraphUri.parse to properly decode repo revision
    const decodedUri = SourcegraphUri.parse(uri.toString())
    const finalUri = new URL(decodedUri.uri)
    finalUri.search = `L${encodeURIComponent(String(startLine))}:${encodeURIComponent(
        String(startChar)
    )}-${encodeURIComponent(String(endLine))}:${encodeURIComponent(String(endChar))}${vsceUtms}`
    return finalUri.href.replace(finalUri.protocol, instanceUrl.protocol)
}
