package com.sourcegraph.cody;

import com.intellij.openapi.util.IconLoader;
import com.intellij.ui.AnimatedIcon;
import javax.swing.*;

public interface Icons {
  Icon CodyLogo = IconLoader.getIcon("/icons/codyLogo.svg", Icons.class);
  Icon HiImCody = IconLoader.getIcon("/icons/hiImCodyLogo.svg", Icons.class);

  interface Repository {
    Icon Indexed = IconLoader.getIcon("/icons/repositoryIndexed.svg", Icons.class);
    Icon Missing = IconLoader.getIcon("/icons/repositoryMissing.svg", Icons.class);
  }

  interface Actions {
    Icon Hide = IconLoader.getIcon("/icons/actions/hide.svg", Icons.class);
  }

  interface StatusBar {
    Icon CompletionInProgress = new AnimatedIcon.Default();
    Icon CodyAvailable = IconLoader.getIcon("/icons/codyLogoMonochromatic.svg", Icons.class);
    Icon CodyAutocompleteDisabled =
        IconLoader.getIcon("/icons/codyLogoMonochromaticMuted.svg", Icons.class);
  }
}
