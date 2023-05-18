import { CodebaseContext } from '../../codebase-context'
import { ContextMessage, getContextMessageWithResponse } from '../../codebase-context/messages'
import { ActiveTextEditorSelection, Editor } from '../../editor'
import { IntentDetector } from '../../intent-detector'
import { MAX_CURRENT_FILE_TOKENS, MAX_HUMAN_INPUT_TOKENS } from '../../prompt/constants'
import {
    populateCurrentEditorContextTemplate,
    populateCurrentEditorSelectedContextTemplate,
} from '../../prompt/templates'
import { truncateText } from '../../prompt/truncation'
import { Interaction } from '../transcript/interaction'

import { Recipe, RecipeContext, RecipeID } from './recipe'

export class ChatQuestion implements Recipe {
    public id: RecipeID = 'chat-question'

    public async getInteraction(humanChatInput: string, context: RecipeContext): Promise<Interaction | null> {
        const truncatedText = truncateText(humanChatInput, MAX_HUMAN_INPUT_TOKENS)

        return Promise.resolve(
            new Interaction(
                { speaker: 'human', text: truncatedText, displayText: humanChatInput },
                { speaker: 'assistant' },
                this.getContextMessages(
                    truncatedText,
                    context.editor,
                    context.intentDetector,
                    context.codebaseContext,
                    context.editor.getActiveTextEditorSelection() || null
                )
            )
        )
    }

    private async getContextMessages(
        text: string,
        editor: Editor,
        intentDetector: IntentDetector,
        codebaseContext: CodebaseContext,
        selection: ActiveTextEditorSelection | null
    ): Promise<ContextMessage[]> {
        const contextMessages: ContextMessage[] = []

        // Add selected text as context when available
        if (selection?.selectedText) {
            contextMessages.push(...ChatQuestion.getEditorSelectionContext(selection))
        }

        const isCodebaseContextRequired = await intentDetector.isCodebaseContextRequired(text)
        if (isCodebaseContextRequired) {
            const codebaseContextMessages = await codebaseContext.getContextMessages(text, {
                numCodeResults: 12,
                numTextResults: 3,
            })
            contextMessages.push(...codebaseContextMessages)
        }

        if (isCodebaseContextRequired || intentDetector.isEditorContextRequired(text)) {
            contextMessages.push(...ChatQuestion.getEditorContext(editor))
        }

        return contextMessages
    }

    public static getEditorContext(editor: Editor): ContextMessage[] {
        const visibleContent = editor.getActiveTextEditorVisibleContent()
        if (!visibleContent) {
            return []
        }
        const truncatedContent = truncateText(visibleContent.content, MAX_CURRENT_FILE_TOKENS)
        return getContextMessageWithResponse(
            populateCurrentEditorContextTemplate(truncatedContent, visibleContent.fileName),
            visibleContent
        )
    }

    public static getEditorSelectionContext(selection: ActiveTextEditorSelection): ContextMessage[] {
        const truncatedContent = truncateText(selection.selectedText, MAX_CURRENT_FILE_TOKENS)
        return getContextMessageWithResponse(
            populateCurrentEditorSelectedContextTemplate(truncatedContent, selection.fileName),
            selection
        )
    }
}
