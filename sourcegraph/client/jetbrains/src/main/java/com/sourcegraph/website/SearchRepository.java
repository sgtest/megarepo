package com.sourcegraph.website;

import com.intellij.openapi.actionSystem.AnActionEvent;
import org.jetbrains.annotations.NotNull;


public class SearchRepository extends SearchActionBase {
    @Override
    public void actionPerformed(@NotNull AnActionEvent event) {
        super.actionPerformedMode(event, Scope.REPOSITORY);
    }
}
