import { render } from '@testing-library/react';
import React from 'react';
import { Route } from 'react-router-dom';
import { byRole, byTestId } from 'testing-library-selector';

import { selectors } from '@grafana/e2e-selectors';
import { locationService } from '@grafana/runtime';
import RuleEditor from 'app/features/alerting/unified/RuleEditor';

import { TestProvider } from './TestProvider';

export const ui = {
  inputs: {
    name: byRole('textbox', { name: 'name' }),
    alertType: byTestId('alert-type-picker'),
    dataSource: byTestId('datasource-picker'),
    folder: byTestId('folder-picker'),
    folderContainer: byTestId(selectors.components.FolderPicker.containerV2),
    namespace: byTestId('namespace-picker'),
    group: byTestId('group-picker'),
    annotationKey: (idx: number) => byTestId(`annotation-key-${idx}`),
    annotationValue: (idx: number) => byTestId(`annotation-value-${idx}`),
    labelKey: (idx: number) => byTestId(`label-key-${idx}`),
    labelValue: (idx: number) => byTestId(`label-value-${idx}`),
    expr: byTestId('expr'),
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
