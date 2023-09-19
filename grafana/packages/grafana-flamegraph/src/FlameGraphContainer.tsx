import { css } from '@emotion/css';
import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useMeasure } from 'react-use';

import { DataFrame, GrafanaTheme2 } from '@grafana/data';

import FlameGraph from './FlameGraph/FlameGraph';
import { FlameGraphDataContainer } from './FlameGraph/dataTransform';
import FlameGraphHeader from './FlameGraphHeader';
import FlameGraphTopTableContainer from './TopTable/FlameGraphTopTableContainer';
import { MIN_WIDTH_TO_SHOW_BOTH_TOPTABLE_AND_FLAMEGRAPH } from './constants';
import { ClickedItemData, ColorScheme, ColorSchemeDiff, SelectedView, TextAlign } from './types';

export type Props = {
  /**
   * DataFrame with the profile data. The dataFrame needs to have the following fields:
   * label: string - the label of the node
   * level: number - the nesting level of the node
   * value: number - the total value of the node
   * self: number - the self value of the node
   * Optionally if it represents diff of 2 different profiles it can also have fields:
   * valueRight: number - the total value of the node in the right profile
   * selfRight: number - the self value of the node in the right profile
   */
  data?: DataFrame;

  /**
   * Whether the header should be sticky and be always visible on the top when scrolling.
   */
  stickyHeader?: boolean;

  /**
   * Provides a theme for the visualization on which colors and some sizes are based.
   */
  getTheme: () => GrafanaTheme2;

  /**
   * Various interaction hooks that can be used to report on the interaction.
   */
  onTableSymbolClick?: (symbol: string) => void;
  onViewSelected?: (view: string) => void;
  onTextAlignSelected?: (align: string) => void;
  onTableSort?: (sort: string) => void;

  /**
   * Elements that will be shown in the header on the right side of the header buttons. Useful for additional
   * functionality.
   */
  extraHeaderElements?: React.ReactNode;

  /**
   * If true the flamegraph will be rendered on top of the table.
   */
  vertical?: boolean;
};

const FlameGraphContainer = ({
  data,
  onTableSymbolClick,
  onViewSelected,
  onTextAlignSelected,
  onTableSort,
  getTheme,
  stickyHeader,
  extraHeaderElements,
  vertical,
}: Props) => {
  const [focusedItemData, setFocusedItemData] = useState<ClickedItemData>();

  const [rangeMin, setRangeMin] = useState(0);
  const [rangeMax, setRangeMax] = useState(1);
  const [search, setSearch] = useState('');
  const [selectedView, setSelectedView] = useState(SelectedView.Both);
  const [sizeRef, { width: containerWidth }] = useMeasure<HTMLDivElement>();
  const [textAlign, setTextAlign] = useState<TextAlign>('left');
  // This is a label of the item because in sandwich view we group all items by label and present a merged graph
  const [sandwichItem, setSandwichItem] = useState<string>();

  const theme = getTheme();

  const dataContainer = useMemo((): FlameGraphDataContainer | undefined => {
    if (!data) {
      return;
    }
    return new FlameGraphDataContainer(data, theme);
  }, [data, theme]);

  const [colorScheme, setColorScheme] = useColorScheme(dataContainer);
  const styles = getStyles(theme, vertical);

  // If user resizes window with both as the selected view
  useEffect(() => {
    if (
      containerWidth > 0 &&
      containerWidth < MIN_WIDTH_TO_SHOW_BOTH_TOPTABLE_AND_FLAMEGRAPH &&
      selectedView === SelectedView.Both &&
      !vertical
    ) {
      setSelectedView(SelectedView.FlameGraph);
    }
  }, [selectedView, setSelectedView, containerWidth, vertical]);

  const resetFocus = useCallback(() => {
    setFocusedItemData(undefined);
    setRangeMin(0);
    setRangeMax(1);
  }, [setFocusedItemData, setRangeMax, setRangeMin]);

  function resetSandwich() {
    setSandwichItem(undefined);
  }

  useEffect(() => {
    resetFocus();
    resetSandwich();
  }, [data, resetFocus]);

  const onSymbolClick = useCallback(
    (symbol: string) => {
      if (search === symbol) {
        setSearch('');
      } else {
        onTableSymbolClick?.(symbol);
        setSearch(symbol);
        resetFocus();
      }
    },
    [setSearch, resetFocus, onTableSymbolClick, search]
  );

  if (!dataContainer) {
    return null;
  }

  return (
    <div ref={sizeRef} className={styles.container}>
      <FlameGraphHeader
        search={search}
        setSearch={setSearch}
        selectedView={selectedView}
        setSelectedView={(view) => {
          setSelectedView(view);
          onViewSelected?.(view);
        }}
        containerWidth={containerWidth}
        onReset={() => {
          resetFocus();
          resetSandwich();
        }}
        textAlign={textAlign}
        onTextAlignChange={(align) => {
          setTextAlign(align);
          onTextAlignSelected?.(align);
        }}
        showResetButton={Boolean(focusedItemData || sandwichItem)}
        colorScheme={colorScheme}
        onColorSchemeChange={setColorScheme}
        stickyHeader={Boolean(stickyHeader)}
        extraHeaderElements={extraHeaderElements}
        vertical={vertical}
        isDiffMode={Boolean(dataContainer.isDiffFlamegraph())}
        getTheme={getTheme}
      />

      <div className={styles.body}>
        {selectedView !== SelectedView.FlameGraph && (
          <FlameGraphTopTableContainer
            data={dataContainer}
            onSymbolClick={onSymbolClick}
            height={selectedView === SelectedView.TopTable ? 600 : undefined}
            search={search}
            sandwichItem={sandwichItem}
            onSandwich={setSandwichItem}
            onSearch={setSearch}
            onTableSort={onTableSort}
            getTheme={getTheme}
            vertical={vertical}
          />
        )}

        {selectedView !== SelectedView.TopTable && (
          <FlameGraph
            getTheme={getTheme}
            data={dataContainer}
            rangeMin={rangeMin}
            rangeMax={rangeMax}
            search={search}
            setRangeMin={setRangeMin}
            setRangeMax={setRangeMax}
            onItemFocused={(data) => setFocusedItemData(data)}
            focusedItemData={focusedItemData}
            textAlign={textAlign}
            sandwichItem={sandwichItem}
            onSandwich={(label: string) => {
              resetFocus();
              setSandwichItem(label);
            }}
            onFocusPillClick={resetFocus}
            onSandwichPillClick={resetSandwich}
            colorScheme={colorScheme}
          />
        )}
      </div>
    </div>
  );
};

function useColorScheme(dataContainer: FlameGraphDataContainer | undefined) {
  const [colorScheme, setColorScheme] = useState<ColorScheme | ColorSchemeDiff>(
    dataContainer?.isDiffFlamegraph() ? ColorSchemeDiff.Default : ColorScheme.ValueBased
  );
  useEffect(() => {
    if (
      dataContainer?.isDiffFlamegraph() &&
      (colorScheme === ColorScheme.ValueBased || colorScheme === ColorScheme.PackageBased)
    ) {
      setColorScheme(ColorSchemeDiff.Default);
    }

    if (
      !dataContainer?.isDiffFlamegraph() &&
      (colorScheme === ColorSchemeDiff.Default || colorScheme === ColorSchemeDiff.DiffColorBlind)
    ) {
      setColorScheme(ColorScheme.ValueBased);
    }
  }, [dataContainer, colorScheme]);

  return [colorScheme, setColorScheme] as const;
}

function getStyles(theme: GrafanaTheme2, vertical?: boolean) {
  return {
    container: css({
      label: 'container',
      height: '100%',
      display: 'flex',
      flex: '1 1 0',
      flexDirection: 'column',
      minHeight: 0,
      gap: theme.spacing(1),
    }),
    body: css({
      label: 'body',
      display: 'flex',
      flexGrow: 1,
      minHeight: 0,
      flexDirection: vertical ? 'column-reverse' : 'row',
    }),
  };
}

export default FlameGraphContainer;
