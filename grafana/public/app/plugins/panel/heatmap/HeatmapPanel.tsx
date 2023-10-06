import { css } from '@emotion/css';
import React, { useCallback, useMemo, useRef, useState } from 'react';

import { DataFrame, DataFrameType, Field, getLinksSupplier, GrafanaTheme2, PanelProps, TimeRange } from '@grafana/data';
import { PanelDataErrorView } from '@grafana/runtime';
import { ScaleDistributionConfig } from '@grafana/schema';
import {
  Portal,
  ScaleDistribution,
  UPlotChart,
  usePanelContext,
  useStyles2,
  useTheme2,
  VizLayout,
  VizTooltipContainer,
} from '@grafana/ui';
import { ColorScale } from 'app/core/components/ColorScale/ColorScale';
import { isHeatmapCellsDense, readHeatmapRowsCustomMeta } from 'app/features/transformers/calculateHeatmap/heatmap';

import { ExemplarModalHeader } from './ExemplarModalHeader';
import { HeatmapHoverView } from './HeatmapHoverView';
import { prepareHeatmapData } from './fields';
import { quantizeScheme } from './palettes';
import { Options } from './types';
import { HeatmapHoverEvent, prepConfig } from './utils';

interface HeatmapPanelProps extends PanelProps<Options> {}

export const HeatmapPanel = ({
  data,
  id,
  timeRange,
  timeZone,
  width,
  height,
  options,
  fieldConfig,
  eventBus,
  onChangeTimeRange,
  replaceVariables,
}: HeatmapPanelProps) => {
  const theme = useTheme2();
  const styles = useStyles2(getStyles);
  const { sync } = usePanelContext();

  //  necessary for enabling datalinks in hover view
  let scopedVarsFromRawData = [];
  for (const series of data.series) {
    for (const field of series.fields) {
      if (field.state?.scopedVars) {
        scopedVarsFromRawData.push(field.state.scopedVars);
      }
    }
  }

  // ugh
  let timeRangeRef = useRef<TimeRange>(timeRange);
  timeRangeRef.current = timeRange;

  const getFieldLinksSupplier = useCallback(
    (exemplars: DataFrame, field: Field) => {
      return getLinksSupplier(exemplars, field, field.state?.scopedVars ?? {}, replaceVariables);
    },
    [replaceVariables]
  );

  const palette = useMemo(() => quantizeScheme(options.color, theme), [options.color, theme]);

  const info = useMemo(() => {
    try {
      return prepareHeatmapData(
        data.series,
        data.annotations,
        options,
        palette,
        theme,
        getFieldLinksSupplier,
        replaceVariables
      );
    } catch (ex) {
      return { warning: `${ex}` };
    }
  }, [data.series, data.annotations, options, palette, theme, getFieldLinksSupplier, replaceVariables]);

  const facets = useMemo(() => {
    let exemplarsXFacet: number[] | undefined = []; // "Time" field
    let exemplarsyFacet: Array<number | undefined> = [];

    const meta = readHeatmapRowsCustomMeta(info.heatmap);
    if (info.exemplars?.length && meta.yMatchWithLabel) {
      exemplarsXFacet = info.exemplars?.fields[0].values;

      // ordinal/labeled heatmap-buckets?
      const hasLabeledY = meta.yOrdinalDisplay != null;

      if (hasLabeledY) {
        let matchExemplarsBy = info.exemplars?.fields.find((field) => field.name === meta.yMatchWithLabel)!.values;
        exemplarsyFacet = matchExemplarsBy.map((label) => meta.yOrdinalLabel?.indexOf(label));
      } else {
        exemplarsyFacet = info.exemplars?.fields[1].values; // "Value" field
      }
    }

    return [null, info.heatmap?.fields.map((f) => f.values), [exemplarsXFacet, exemplarsyFacet]];
  }, [info.heatmap, info.exemplars]);

  const [hover, setHover] = useState<HeatmapHoverEvent | undefined>(undefined);
  const [shouldDisplayCloseButton, setShouldDisplayCloseButton] = useState<boolean>(false);
  const isToolTipOpen = useRef<boolean>(false);

  const onCloseToolTip = () => {
    isToolTipOpen.current = false;
    setShouldDisplayCloseButton(false);
    onhover(null);
  };

  const onclick = () => {
    isToolTipOpen.current = !isToolTipOpen.current;

    // Linking into useState required to re-render tooltip
    setShouldDisplayCloseButton(isToolTipOpen.current);
  };

  const onhover = useCallback(
    (evt?: HeatmapHoverEvent | null) => {
      setHover(evt ?? undefined);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [options, data.structureRev]
  );

  // ugh
  const dataRef = useRef(info);
  dataRef.current = info;

  const builder = useMemo(() => {
    const scaleConfig: ScaleDistributionConfig = dataRef.current?.heatmap?.fields[1].config?.custom?.scaleDistribution;

    return prepConfig({
      dataRef,
      theme,
      eventBus,
      onhover: onhover,
      onclick: options.tooltip.show ? onclick : null,
      onzoom: (evt) => {
        const delta = evt.xMax - evt.xMin;
        if (delta > 1) {
          onChangeTimeRange({ from: evt.xMin, to: evt.xMax });
        }
      },
      isToolTipOpen,
      timeZone,
      getTimeRange: () => timeRangeRef.current,
      sync,
      cellGap: options.cellGap,
      hideLE: options.filterValues?.le,
      hideGE: options.filterValues?.ge,
      exemplarColor: options.exemplars?.color ?? 'rgba(255,0,255,0.7)',
      yAxisConfig: options.yAxis,
      ySizeDivisor: scaleConfig?.type === ScaleDistribution.Log ? +(options.calculation?.yBuckets?.value || 1) : 1,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options, timeZone, data.structureRev]);

  const renderLegend = () => {
    if (!info.heatmap || !options.legend.show) {
      return null;
    }

    let heatmapType = dataRef.current?.heatmap?.meta?.type;
    let isSparseHeatmap = heatmapType === DataFrameType.HeatmapCells && !isHeatmapCellsDense(dataRef.current?.heatmap!);
    let countFieldIdx = !isSparseHeatmap ? 2 : 3;
    const countField = info.heatmap.fields[countFieldIdx];

    let hoverValue: number | undefined = undefined;
    // seriesIdx: 1 is heatmap layer; 2 is exemplar layer
    if (hover && info.heatmap.fields && hover.seriesIdx === 1) {
      hoverValue = countField.values[hover.dataIdx];
    }

    return (
      <VizLayout.Legend placement="bottom" maxHeight="20%">
        <div className={styles.colorScaleWrapper}>
          <ColorScale
            hoverValue={hoverValue}
            colorPalette={palette}
            min={dataRef.current.heatmapColors?.minValue!}
            max={dataRef.current.heatmapColors?.maxValue!}
            display={info.display}
          />
        </div>
      </VizLayout.Legend>
    );
  };

  if (info.warning || !info.heatmap) {
    return (
      <PanelDataErrorView
        panelId={id}
        fieldConfig={fieldConfig}
        data={data}
        needsNumberField={true}
        message={info.warning}
      />
    );
  }

  return (
    <>
      <VizLayout width={width} height={height} legend={renderLegend()}>
        {(vizWidth: number, vizHeight: number) => (
          <UPlotChart config={builder} data={facets as any} width={vizWidth} height={vizHeight}>
            {/*children ? children(config, alignedFrame) : null*/}
          </UPlotChart>
        )}
      </VizLayout>
      <Portal>
        {hover && options.tooltip.show && (
          <VizTooltipContainer
            position={{ x: hover.pageX, y: hover.pageY }}
            offset={{ x: 10, y: 10 }}
            allowPointerEvents={isToolTipOpen.current}
          >
            {shouldDisplayCloseButton && <ExemplarModalHeader onClick={onCloseToolTip} />}
            <HeatmapHoverView
              timeRange={timeRange}
              data={info}
              hover={hover}
              showHistogram={options.tooltip.yHistogram}
              replaceVars={replaceVariables}
              scopedVars={scopedVarsFromRawData}
            />
          </VizTooltipContainer>
        )}
      </Portal>
    </>
  );
};

const getStyles = (theme: GrafanaTheme2) => ({
  colorScaleWrapper: css`
    margin-left: 25px;
    padding: 10px 0;
    max-width: 300px;
  `,
});
