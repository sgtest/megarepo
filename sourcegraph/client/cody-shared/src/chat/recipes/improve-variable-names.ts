import { CodebaseContext } from '../../codebase-context'
import { Editor } from '../../editor'
import { IntentDetector } from '../../intent-detector'
import { MAX_RECIPE_INPUT_TOKENS, MAX_RECIPE_SURROUNDING_TOKENS } from '../../prompt/constants'
import { truncateText, truncateTextStart } from '../../prompt/truncation'
import { getShortTimestamp } from '../../timestamp'
import { renderMarkdown } from '../markdown'
import { Interaction } from '../transcript/interaction'

import {
    MARKDOWN_FORMAT_PROMPT,
    getNormalizedLanguageName,
    getContextMessagesFromSelection,
    getFileExtension,
} from './helpers'
import { Recipe } from './recipe'

export class ImproveVariableNames implements Recipe {
    public getID(): string {
        return 'improve-variable-names'
    }

    public async getInteraction(
        _humanChatInput: string,
        editor: Editor,
        _intentDetector: IntentDetector,
        codebaseContext: CodebaseContext
    ): Promise<Interaction | null> {
        const selection = editor.getActiveTextEditorSelection()
        if (!selection) {
            return Promise.resolve(null)
        }

        const timestamp = getShortTimestamp()
        const truncatedSelectedText = truncateText(selection.selectedText, MAX_RECIPE_INPUT_TOKENS)
        const truncatedPrecedingText = truncateTextStart(selection.precedingText, MAX_RECIPE_SURROUNDING_TOKENS)
        const truncatedFollowingText = truncateText(selection.followingText, MAX_RECIPE_SURROUNDING_TOKENS)
        const extension = getFileExtension(selection.fileName)

        const displayText = renderMarkdown(
            `Improve the variable names in the following code:\n\`\`\`\n${selection.selectedText}\n\`\`\``
        )

        const languageName = getNormalizedLanguageName(selection.fileName)
        const promptMessage = `Improve the variable names in this ${languageName} code by replacing the variable names with new identifiers which succinctly capture the purpose of the variable. We want the new code to be a drop-in replacement, so do not change names bound outside the scope of this code, like function names or members defined elsewhere. Only change the names of local variables and parameters:\n\n\`\`\`${extension}\n${truncatedSelectedText}\n\`\`\`\n${MARKDOWN_FORMAT_PROMPT}`
        const assistantResponsePrefix = `Here is the improved code:\n\`\`\`${extension}\n`

        return new Interaction(
            { speaker: 'human', text: promptMessage, displayText, timestamp },
            {
                speaker: 'assistant',
                prefix: assistantResponsePrefix,
                text: assistantResponsePrefix,
                displayText: '',
                timestamp,
            },
            getContextMessagesFromSelection(
                truncatedSelectedText,
                truncatedPrecedingText,
                truncatedFollowingText,
                selection.fileName,
                codebaseContext
            )
        )
    }
}
