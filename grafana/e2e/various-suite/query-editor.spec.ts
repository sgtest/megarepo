import { e2e } from '../utils';

describe('Query editor', () => {
  beforeEach(() => {
    e2e.flows.login(Cypress.env('USERNAME'), Cypress.env('PASSWORD'));
  });

  it('Undo should work in query editor for prometheus -- test CI.', () => {
    e2e.pages.Explore.visit();
    e2e.components.DataSourcePicker.container().should('be.visible').click();

    cy.contains('gdev-prometheus').scrollIntoView().should('be.visible').click();
    const queryText = `rate(http_requests_total{job="grafana"}[5m])`;

    e2e.components.RadioButton.container().filter(':contains("Code")').click();

    // we need to wait for the query-field being lazy-loaded, in two steps:
    // it is a two-step process:
    // 1. first we wait for the text 'Loading...' to appear
    // 1. then we wait for the text 'Loading...' to disappear
    const monacoLoadingText = 'Loading...';
    e2e.components.QueryField.container().should('be.visible').should('have.text', monacoLoadingText);
    e2e.components.QueryField.container().should('be.visible').should('not.have.text', monacoLoadingText);
    e2e.components.QueryField.container().type(queryText, { parseSpecialCharSequences: false }).type('{backspace}');

    cy.contains(queryText.slice(0, -1)).should('be.visible');

    e2e.components.QueryField.container().type(e2e.typings.undo());

    cy.contains(queryText).should('be.visible');

    e2e.components.Alert.alertV2('error').should('not.be.visible');
  });
});
