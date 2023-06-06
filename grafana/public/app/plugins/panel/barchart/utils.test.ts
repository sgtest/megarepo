import { assertIsDefined } from 'test/helpers/asserts';

import {
  createTheme,
  DefaultTimeZone,
  EventBusSrv,
  FieldConfig,
  FieldType,
  getDefaultTimeRange,
  MutableDataFrame,
  VizOrientation,
} from '@grafana/data';
import {
  LegendDisplayMode,
  TooltipDisplayMode,
  VisibilityMode,
  GraphGradientMode,
  StackingMode,
  SortOrder,
} from '@grafana/schema';

import { FieldConfig as PanelFieldConfig, Options } from './panelcfg.gen';
import { BarChartOptionsEX, prepareBarChartDisplayValues, preparePlotConfigBuilder } from './utils';

function mockDataFrame() {
  const df1 = new MutableDataFrame({
    refId: 'A',
    fields: [{ name: 'ts', type: FieldType.string, values: ['a', 'b', 'c'] }],
  });

  const df2 = new MutableDataFrame({
    refId: 'B',
    fields: [{ name: 'ts', type: FieldType.time, values: [1, 2, 4] }],
  });

  const f1Config: FieldConfig<PanelFieldConfig> = {
    displayName: 'Metric 1',
    decimals: 2,
    unit: 'm/s',
    custom: {
      gradientMode: GraphGradientMode.Opacity,
      lineWidth: 2,
      fillOpacity: 0.1,
    },
  };

  const f2Config: FieldConfig<PanelFieldConfig> = {
    displayName: 'Metric 2',
    decimals: 2,
    unit: 'kWh',
    custom: {
      gradientMode: GraphGradientMode.Hue,
      lineWidth: 2,
      fillOpacity: 0.1,
    },
  };

  df1.addField({
    name: 'metric1',
    type: FieldType.number,
    config: f1Config,
    state: {},
  });

  df2.addField({
    name: 'metric2',
    type: FieldType.number,
    config: f2Config,
    state: {},
  });

  const info = prepareBarChartDisplayValues([df1], createTheme(), {} as Options);

  if (!('aligned' in info)) {
    throw new Error('Bar chart not prepared correctly');
  }

  return info.aligned;
}

jest.mock('@grafana/data', () => ({
  ...jest.requireActual('@grafana/data'),
  DefaultTimeZone: 'utc',
}));

describe('BarChart utils', () => {
  describe('preparePlotConfigBuilder', () => {
    const frame = mockDataFrame();

    const config: BarChartOptionsEX = {
      orientation: VizOrientation.Auto,
      groupWidth: 20,
      barWidth: 2,
      showValue: VisibilityMode.Always,
      legend: {
        displayMode: LegendDisplayMode.List,
        showLegend: true,
        placement: 'bottom',
        calcs: [],
      },
      xTickLabelRotation: 0,
      xTickLabelMaxLength: 20,
      stacking: StackingMode.None,
      tooltip: {
        mode: TooltipDisplayMode.None,
        sort: SortOrder.None,
      },
      text: {
        valueSize: 10,
      },
      fullHighlight: false,
      rawValue: (seriesIdx: number, valueIdx: number) => frame.fields[seriesIdx].values[valueIdx],
    };

    it.each([VizOrientation.Auto, VizOrientation.Horizontal, VizOrientation.Vertical])('orientation', (v) => {
      const result = preparePlotConfigBuilder({
        ...config,
        orientation: v,
        frame: frame!,
        theme: createTheme(),
        timeZones: [DefaultTimeZone],
        getTimeRange: getDefaultTimeRange,
        eventBus: new EventBusSrv(),
        allFrames: [frame],
      }).getConfig();
      expect(result).toMatchSnapshot();
    });

    it.each([VisibilityMode.Always, VisibilityMode.Auto])('value visibility', (v) => {
      expect(
        preparePlotConfigBuilder({
          ...config,
          showValue: v,
          frame: frame!,
          theme: createTheme(),
          timeZones: [DefaultTimeZone],
          getTimeRange: getDefaultTimeRange,
          eventBus: new EventBusSrv(),
          allFrames: [frame],
        }).getConfig()
      ).toMatchSnapshot();
    });

    it.each([StackingMode.None, StackingMode.Percent, StackingMode.Normal])('stacking', (v) => {
      expect(
        preparePlotConfigBuilder({
          ...config,
          stacking: v,
          frame: frame!,
          theme: createTheme(),
          timeZones: [DefaultTimeZone],
          getTimeRange: getDefaultTimeRange,
          eventBus: new EventBusSrv(),
          allFrames: [frame],
        }).getConfig()
      ).toMatchSnapshot();
    });
  });

  describe('prepareGraphableFrames', () => {
    it('will warn when there is no data in the response', () => {
      const result = prepareBarChartDisplayValues([], createTheme(), { stacking: StackingMode.None } as Options);
      const warning = assertIsDefined('warn' in result ? result : null);

      expect(warning.warn).toEqual('No data in response');
    });

    it('will warn when there is no string or time field', () => {
      const df = new MutableDataFrame({
        fields: [
          { name: 'a', type: FieldType.other, values: [1, 2, 3, 4, 5] },
          { name: 'value', values: [1, 2, 3, 4, 5] },
        ],
      });
      const result = prepareBarChartDisplayValues([df], createTheme(), { stacking: StackingMode.None } as Options);
      const warning = assertIsDefined('warn' in result ? result : null);
      expect(warning.warn).toEqual('Bar charts requires a string or time field');
      expect(warning).not.toHaveProperty('viz');
    });

    it('will warn when there are no numeric fields in the response', () => {
      const df = new MutableDataFrame({
        fields: [
          { name: 'a', type: FieldType.string, values: ['a', 'b', 'c', 'd', 'e'] },
          { name: 'value', type: FieldType.boolean, values: [true, true, true, true, true] },
        ],
      });
      const result = prepareBarChartDisplayValues([df], createTheme(), { stacking: StackingMode.None } as Options);
      const warning = assertIsDefined('warn' in result ? result : null);
      expect(warning.warn).toEqual('No numeric fields found');
      expect(warning).not.toHaveProperty('viz');
    });

    it('will convert NaN and Infinty to nulls', () => {
      const df = new MutableDataFrame({
        fields: [
          { name: 'a', type: FieldType.string, values: ['a', 'b', 'c', 'd', 'e'] },
          { name: 'value', values: [-10, NaN, 10, -Infinity, +Infinity] },
        ],
      });
      const result = prepareBarChartDisplayValues([df], createTheme(), { stacking: StackingMode.None } as Options);
      const displayValues = assertIsDefined('viz' in result ? result : null);

      const field = displayValues.viz[0].fields[1];
      expect(field.values).toMatchInlineSnapshot(`
        [
          -10,
          null,
          10,
          null,
          null,
        ]
      `);

      const displayLegendValuesAsc = assertIsDefined('legend' in result ? result : null).legend;
      const legendField = displayLegendValuesAsc.fields[1];

      expect(legendField.values).toMatchInlineSnapshot(`
      [
        -10,
        null,
        10,
        null,
        null,
      ]
    `);
    });

    it('should remove unit from legend values when stacking is percent', () => {
      const frame = new MutableDataFrame({
        fields: [
          { name: 'string', type: FieldType.string, values: ['a', 'b', 'c'] },
          { name: 'a', values: [-10, 20, 10], state: { calcs: { min: -10 } } },
          { name: 'b', values: [20, 20, 20], state: { calcs: { min: 20 } } },
          { name: 'c', values: [10, 10, 10], state: { calcs: { min: 10 } } },
        ],
      });

      const resultAsc = prepareBarChartDisplayValues([frame], createTheme(), {
        stacking: StackingMode.Percent,
      } as Options);
      const displayLegendValuesAsc = assertIsDefined('legend' in resultAsc ? resultAsc : null).legend;

      expect(displayLegendValuesAsc.fields[1].config.unit).toBeUndefined();
      expect(displayLegendValuesAsc.fields[2].config.unit).toBeUndefined();
      expect(displayLegendValuesAsc.fields[3].config.unit).toBeUndefined();
    });
  });
});
