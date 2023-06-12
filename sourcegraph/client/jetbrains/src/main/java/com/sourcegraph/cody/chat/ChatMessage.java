package com.sourcegraph.cody.chat;

import com.sourcegraph.cody.api.Message;
import com.sourcegraph.cody.api.Speaker;
import java.util.ArrayList;
import java.util.List;
import org.jetbrains.annotations.NotNull;

public class ChatMessage extends Message {
  private final @NotNull String displayText;
  private final @NotNull List<String> contextFiles;

  private ChatMessage(
      @NotNull Speaker speaker,
      @NotNull String text,
      @NotNull String displayText,
      @NotNull List<String> contextFiles) {
    super(speaker, text);
    this.displayText = displayText;
    this.contextFiles = contextFiles;
  }

  public static @NotNull ChatMessage createAssistantMessage(@NotNull String text) {
    return new ChatMessage(Speaker.ASSISTANT, text, text, new ArrayList<>());
  }

  public static @NotNull ChatMessage createHumanMessage(
      @NotNull String prompt, @NotNull String displayText, @NotNull List<String> contextFiles) {
    return new ChatMessage(Speaker.HUMAN, prompt, displayText, contextFiles);
  }

  @NotNull
  public String getDisplayText() {
    return displayText;
  }

  @NotNull
  public List<String> getContextFiles() {
    return contextFiles;
  }
}
