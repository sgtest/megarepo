import { FormattingOptions } from '@sqs/jsonc-parser'
import { setProperty } from '@sqs/jsonc-parser/lib/edit'
import { SlackNotificationsConfig } from '../schema/settings.schema'
import { ConfigInsertionFunction } from '../settings/MonacoSettingsEditor'

const defaultFormattingOptions: FormattingOptions = {
    eol: '\n',
    insertSpaces: true,
    tabSize: 2,
}

const addSearchScopeToSettings: ConfigInsertionFunction = config => {
    const value: { name: string; value: string } = {
        name: '<name>',
        value: '<partial query string that will be inserted when the scope is selected>',
    }
    const edits = setProperty(config, ['search.scopes', -1], value, defaultFormattingOptions)
    return { edits, selectText: '<name>' }
}

const addSlackWebhook: ConfigInsertionFunction = config => {
    const value: SlackNotificationsConfig = {
        webhookURL: 'get webhook URL at https://YOUR-WORKSPACE-NAME.slack.com/apps/new/A0F7XDUAZ-incoming-webhooks',
    }
    const edits = setProperty(config, ['notifications.slack'], value, defaultFormattingOptions)
    return { edits, selectText: '""', cursorOffset: 1 }
}

export interface EditorAction {
    id: string
    label: string
    run: ConfigInsertionFunction
}

export const settingsActions: EditorAction[] = [
    { id: 'sourcegraph.settings.searchScopes', label: 'Add search scope', run: addSearchScopeToSettings },
    { id: 'sourcegraph.settings.addSlackWebhook', label: 'Add Slack webhook', run: addSlackWebhook },
]

export const siteConfigActions: EditorAction[] = [

]
