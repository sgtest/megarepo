import { render } from '@testing-library/react';
import { Route } from 'react-router-dom';
import { byRole, byTestId, byText } from 'testing-library-selector';

import { selectors } from '@grafana/e2e-selectors';
import { locationService } from '@grafana/runtime';
import RuleEditor from 'app/features/alerting/unified/RuleEditor';

import { TestProvider } from './TestProvider';

export const ui = {
  loadingIndicator: byText('Loading rule...'),
  inputs: {
    name: byRole('textbox', { name: 'name' }),
    alertType: byTestId('alert-type-picker'),
    dataSource: byTestId(selectors.components.DataSourcePicker.inputV2),
    folder: byTestId('folder-picker'),
    folderContainer: byTestId(selectors.components.FolderPicker.containerV2),
    namespace: byTestId('namespace-picker'),
    group: byTestId('group-picker'),
    annotationKey: (idx: number) => byTestId(`annotation-key-${idx}`),
    annotationValue: (idx: number) => byTestId(`annotation-value-${idx}`),
    labelKey: (idx: number) => byTestId(`label-key-${idx}`),
    labelValue: (idx: number) => byTestId(`label-value-${idx}`),
    expr: byTestId('expr'),
    simplifiedRouting: {
      contactPointRouting: byRole('radio', { name: /select contact point/i }),
      contactPoint: byTestId('contact-point-picker'),
      routingOptions: byText(/muting, grouping and timings \(optional\)/i),
    },
  },
  buttons: {
    saveAndExit: byRole('button', { name: 'Save rule and exit' }),
    save: byRole('button', { name: 'Save rule' }),
    addAnnotation: byRole('button', { name: /Add info/ }),
    addLabel: byRole('button', { name: /Add label/ }),
  },
};

export function renderRuleEditor(identifier?: string, recording = false) {
  if (identifier) {
    locationService.push(`/alerting/${identifier}/edit`);
  } else {
    locationService.push(`/alerting/new/${recording ? 'recording' : 'alerting'}`);
  }

  return render(
    <TestProvider>
      <Route path={['/alerting/new/:type', '/alerting/:id/edit']} component={RuleEditor} />
    </TestProvider>
  );
}
