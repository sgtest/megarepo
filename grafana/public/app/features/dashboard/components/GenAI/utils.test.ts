import { createDashboardModelFixture, createPanelSaveModel } from '../../state/__fixtures__/dashboardFixtures';

import { openai } from './llms';
import { getDashboardChanges, isLLMPluginEnabled, sanitizeReply } from './utils';

// Mock the llms.openai module
jest.mock('./llms', () => ({
  openai: {
    streamChatCompletions: jest.fn(),
    accumulateContent: jest.fn(),
    enabled: jest.fn(),
  },
}));

describe('getDashboardChanges', () => {
  it('should correctly split user changes and migration changes', () => {
    // Mock data for testing
    const deprecatedOptions = {
      legend: { displayMode: 'hidden', showLegend: false },
    };
    const deprecatedVersion = 37;
    const dashboard = createDashboardModelFixture({
      schemaVersion: deprecatedVersion,
      panels: [createPanelSaveModel({ title: 'Panel 1', options: deprecatedOptions })],
    });

    // Update title for the first panel
    dashboard.updatePanels([
      {
        ...dashboard.panels[0],
        title: 'New title',
      },
      ...dashboard.panels.slice(1),
    ]);

    // Call the function to test
    const result = getDashboardChanges(dashboard);

    // Assertions
    expect(result.migrationChanges).toEqual(
      '===================================================================\n' +
        '--- Before migration changes\t\n' +
        '+++ After migration changes\t\n' +
        '@@ -1,9 +1,9 @@\n' +
        ' {\n' +
        '   "editable": true,\n' +
        '   "graphTooltip": 0,\n' +
        '-  "schemaVersion": 37,\n' +
        '+  "schemaVersion": 38,\n' +
        '   "timezone": "",\n' +
        '   "panels": [\n' +
        '     {\n' +
        '       "type": "timeseries",\n' +
        '       "title": "Panel 1",\n'
    );
    expect(result.userChanges).toEqual(
      '===================================================================\n' +
        '--- Before user changes\t\n' +
        '+++ After user changes\t\n' +
        '@@ -3,16 +3,17 @@\n' +
        '   "graphTooltip": 0,\n' +
        '   "schemaVersion": 38,\n' +
        '   "timezone": "",\n' +
        '   "panels": [\n' +
        '     {\n' +
        '-      "type": "timeseries",\n' +
        '-      "title": "Panel 1",\n' +
        '+      "id": 1,\n' +
        '       "options": {\n' +
        '         "legend": {\n' +
        '           "displayMode": "hidden",\n' +
        '           "showLegend": false\n' +
        '         }\n' +
        '-      }\n' +
        '+      },\n' +
        '+      "title": "New title",\n' +
        '+      "type": "timeseries"\n' +
        '     }\n' +
        '   ]\n' +
        ' }\n' +
        '\\ No newline at end of file\n'
    );
    expect(result.migrationChanges).toBeDefined();
  });
});

describe('isLLMPluginEnabled', () => {
  it('should return true if LLM plugin is enabled', async () => {
    // Mock llms.openai.enabled to return true
    jest.mocked(openai.enabled).mockResolvedValue(true);

    const enabled = await isLLMPluginEnabled();

    expect(enabled).toBe(true);
  });

  it('should return false if LLM plugin is not enabled', async () => {
    // Mock llms.openai.enabled to return false
    jest.mocked(openai.enabled).mockResolvedValue(false);

    const enabled = await isLLMPluginEnabled();

    expect(enabled).toBe(false);
  });
});

describe('sanitizeReply', () => {
  it('should remove quotes from the beginning and end of a string', () => {
    expect(sanitizeReply('"Hello, world!"')).toBe('Hello, world!');
  });

  it('should not remove quotes from the middle of a string', () => {
    expect(sanitizeReply('Hello, "world"!')).toBe('Hello, "world"!');
  });

  it('should only remove quotes if they are at the beginning or end of a string, and not in the middle', () => {
    expect(sanitizeReply('"Hello", world!')).toBe('Hello", world!');
  });

  it('should return an empty string if given an empty string', () => {
    expect(sanitizeReply('')).toBe('');
  });
});
