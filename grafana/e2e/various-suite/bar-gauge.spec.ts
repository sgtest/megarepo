import { selectors } from '@grafana/e2e-selectors';

import { e2e } from '../utils';

e2e.scenario({
  describeName: 'Bar Gauge Panel',
  itName: 'Bar Gauge rendering e2e tests',
  addScenarioDataSource: false,
  addScenarioDashBoard: false,
  skipScenario: false,
  scenario: () => {
    // open Panel Tests - Bar Gauge
    e2e.flows.openDashboard({ uid: 'O6f11TZWk' });

    cy.get(`[data-panelid=6] [data-testid^="${selectors.components.Panels.Visualization.BarGauge.valueV2}"]`)
      .should('have.css', 'color', 'rgb(242, 73, 92)')
      .contains('100');
  },
});
