import React, { Component } from 'react';

import { DataFrame, TimeRange } from '@grafana/data';

import { withTheme2 } from '../../themes/ThemeContext';
import { GraphNG, GraphNGProps, PropDiffFn } from '../GraphNG/GraphNG';
import { PanelContext, PanelContextRoot } from '../PanelChrome/PanelContext';
import { hasVisibleLegendSeries, PlotLegend } from '../uPlot/PlotLegend';
import { UPlotConfigBuilder } from '../uPlot/config/UPlotConfigBuilder';

import { preparePlotConfigBuilder } from './utils';

const propsToDiff: Array<string | PropDiffFn> = ['legend', 'options', 'theme'];

type TimeSeriesProps = Omit<GraphNGProps, 'prepConfig' | 'propsToDiff' | 'renderLegend'>;

export class UnthemedTimeSeries extends Component<TimeSeriesProps> {
  static contextType = PanelContextRoot;
  panelContext: PanelContext = {} as PanelContext;

  prepConfig = (alignedFrame: DataFrame, allFrames: DataFrame[], getTimeRange: () => TimeRange) => {
    const { eventBus, eventsScope, sync } = this.context as PanelContext;
    const { theme, timeZone, renderers, tweakAxis, tweakScale } = this.props;

    return preparePlotConfigBuilder({
      frame: alignedFrame,
      theme,
      timeZones: Array.isArray(timeZone) ? timeZone : [timeZone],
      getTimeRange,
      eventBus,
      sync,
      allFrames,
      renderers,
      tweakScale,
      tweakAxis,
      eventsScope,
    });
  };

  renderLegend = (config: UPlotConfigBuilder) => {
    const { legend, frames } = this.props;

    if (!config || (legend && !legend.showLegend) || !hasVisibleLegendSeries(config, frames)) {
      return null;
    }

    return <PlotLegend data={frames} config={config} {...legend} />;
  };

  render() {
    return (
      <GraphNG
        {...this.props}
        prepConfig={this.prepConfig}
        propsToDiff={propsToDiff}
        renderLegend={this.renderLegend}
      />
    );
  }
}

export const TimeSeries = withTheme2(UnthemedTimeSeries);
TimeSeries.displayName = 'TimeSeries';
