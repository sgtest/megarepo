package com.sourcegraph.cody.autocomplete;

import com.intellij.openapi.actionSystem.DataContext;
import com.intellij.openapi.application.WriteAction;
import com.intellij.openapi.editor.*;
import com.intellij.openapi.editor.actionSystem.EditorAction;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.TextRange;
import com.sourcegraph.cody.agent.CodyAgent;
import com.sourcegraph.cody.agent.CodyAgentServer;
import com.sourcegraph.cody.vscode.InlineAutocompleteItem;
import com.sourcegraph.telemetry.GraphQlLogger;
import com.sourcegraph.utils.CodyEditorUtil;
import java.util.List;
import java.util.Optional;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

/**
 * The action that gets triggered when the user accepts a Cody completion.
 *
 * <p>The action works by reading the Inlay at the caret position and inserting the completion text
 * into the editor.
 */
public class AcceptCodyAutocompleteAction extends EditorAction {
  public AcceptCodyAutocompleteAction() {
    super(new AcceptCompletionActionHandler());
  }

  private static class AcceptCompletionActionHandler extends AutocompleteActionHandler {

    /**
     * Applies the autocomplete to the document at a caret: 1. Replaces the string between the caret
     * offset and its line end with the current completion 2. Moves the caret to the start and end
     * offsets with the completion text. If there are multiple carets, uses the first one. If there
     * are no completions at the caret, does nothing.
     */
    @Override
    protected void doExecute(
        @NotNull Editor editor, @Nullable Caret maybeCaret, @Nullable DataContext dataContext) {
      final Project project = editor.getProject();
      if (project == null) {
        return;
      }

      CodyAgentServer server = CodyAgent.getServer(project);
      boolean isAgentCompletion = server != null;

      if (isAgentCompletion) {
        AutocompleteTelemetry telemetry =
            CodyAutocompleteManager.getInstance().getCurrentAutocompleteTelemetry();
        GraphQlLogger.logAutocompleteAcceptedEvent(
            project, telemetry != null ? telemetry.params() : null);
        server.autocompleteClearLastCandidate();
        acceptAgentAutocomplete(editor, maybeCaret);
      } else {
        Optional.ofNullable(maybeCaret)
            .or(() -> getCaret(editor))
            .flatMap(AutocompleteText::atCaret)
            .ifPresent(
                autoComplete -> {
                  /* Log the event */
                  GraphQlLogger.logCodyEvent(project, "completion", "accepted");

                  WriteAction.run(() -> applyAutocomplete(editor.getDocument(), autoComplete));
                });
      }
    }

    private void acceptAgentAutocomplete(@NotNull Editor editor, @Nullable Caret maybeCaret) {
      Caret caret = Optional.ofNullable(maybeCaret).or(() -> getCaret(editor)).orElse(null);
      if (caret == null) {
        return;
      }
      InlineAutocompleteItem completionItem = getAgentAutocompleteItem(caret);
      if (completionItem == null) {
        return;
      }
      WriteAction.run(() -> applyInsertText(editor, caret, completionItem));
    }

    @NotNull
    private static Optional<Caret> getCaret(@NotNull Editor editor) {
      List<Caret> allCarets = editor.getCaretModel().getAllCarets();
      if (allCarets.size() < 2) { // Only accept completion if there's a single caret.
        return allCarets.stream().findFirst();
      } else {
        return Optional.empty();
      }
    }

    private static void applyInsertText(
        @NotNull Editor editor,
        @NotNull Caret caret,
        @NotNull InlineAutocompleteItem completionItem) {
      Document document = editor.getDocument();
      TextRange range = CodyEditorUtil.getTextRange(document, completionItem.range);
      document.replaceString(
          range.getStartOffset(), range.getEndOffset(), completionItem.insertText);
      caret.moveToOffset(range.getStartOffset() + completionItem.insertText.length());
      editor.getScrollingModel().scrollToCaret(ScrollType.MAKE_VISIBLE);
    }
  }

  /**
   * Applies the autocomplete to the document at a caret. This replaces the string between the caret
   * offset and its line end with the autocompletion string and then moves the caret to the end of
   * the autocompletion.
   *
   * @param document the document to apply the autocomplete to
   * @param autoComplete the actual autocomplete text along with the corresponding caret
   */
  private static void applyAutocomplete(
      @NotNull Document document, @NotNull AutocompleteTextAtCaret autoComplete) {
    // Calculate the end of the line to replace
    int lineEndOffset =
        document.getLineEndOffset(document.getLineNumber(autoComplete.caret.getOffset()));

    // Get autocompletion string
    String autoCompletionString =
        autoComplete.autoCompleteText.getAutoCompletionString(
            document.getText(TextRange.create(autoComplete.caret.getOffset(), lineEndOffset)));

    // If the autocompletion string does not contain the suffix of the line, add it to the end
    String sameLineSuffix =
        document.getText(TextRange.create(autoComplete.caret.getOffset(), lineEndOffset));
    String sameLineSuffixIfMissing =
        autoCompletionString.contains(sameLineSuffix) ? "" : sameLineSuffix;

    // Replace the line with the autocompletion string
    String finalAutoCompletionString = autoCompletionString + sameLineSuffixIfMissing;
    document.replaceString(
        autoComplete.caret.getOffset(), lineEndOffset, finalAutoCompletionString);

    // Move the caret to the end of the autocompletion string
    autoComplete.caret.moveToOffset(
        autoComplete.caret.getOffset() + finalAutoCompletionString.length());
  }
}
