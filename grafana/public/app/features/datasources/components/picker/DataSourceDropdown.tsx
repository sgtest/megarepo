import { css } from '@emotion/css';
import { useDialog } from '@react-aria/dialog';
import { useOverlay } from '@react-aria/overlays';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import { usePopper } from 'react-popper';

import { DataSourceInstanceSettings, GrafanaTheme2 } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';
import { reportInteraction } from '@grafana/runtime';
import { DataQuery, DataSourceJsonData } from '@grafana/schema';
import { Button, CustomScrollbar, Icon, Input, ModalsController, Portal, useStyles2 } from '@grafana/ui';
import config from 'app/core/config';
import { useKeyNavigationListener } from 'app/features/search/hooks/useSearchKeyboardSelection';
import { defaultFileUploadQuery, GrafanaQuery } from 'app/plugins/datasource/grafana/types';

import { useDatasource } from '../../hooks';

import { DataSourceList } from './DataSourceList';
import { DataSourceLogo, DataSourceLogoPlaceHolder } from './DataSourceLogo';
import { DataSourceModal } from './DataSourceModal';
import { applyMaxSize, maxSize } from './popperModifiers';
import { PickerContentProps, DataSourceDropdownProps } from './types';
import { dataSourceLabel, matchDataSourceWithSearch } from './utils';

const INTERACTION_EVENT_NAME = 'dashboards_dspicker_clicked';
const INTERACTION_ITEM = {
  OPEN_DROPDOWN: 'open_dspicker',
  SELECT_DS: 'select_ds',
  ADD_FILE: 'add_file',
  OPEN_ADVANCED_DS_PICKER: 'open_advanced_ds_picker',
  CONFIG_NEW_DS_EMPTY_STATE: 'config_new_ds_empty_state',
};

export function DataSourceDropdown(props: DataSourceDropdownProps) {
  const { current, onChange, hideTextValue, width, ...restProps } = props;

  const [isOpen, setOpen] = useState(false);
  const [inputHasFocus, setInputHasFocus] = useState(false);
  const [markerElement, setMarkerElement] = useState<HTMLInputElement | null>();
  const [selectorElement, setSelectorElement] = useState<HTMLDivElement | null>();
  const [filterTerm, setFilterTerm] = useState<string>('');
  const openDropdown = () => {
    reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.OPEN_DROPDOWN });
    setOpen(true);
    markerElement?.focus();
  };

  const { onKeyDown, keyboardEvents } = useKeyNavigationListener();

  useEffect(() => {
    const sub = keyboardEvents.subscribe({
      next: (keyEvent) => {
        switch (keyEvent?.code) {
          case 'ArrowDown': {
            openDropdown();
            keyEvent.preventDefault();
            break;
          }
          case 'ArrowUp':
            openDropdown();
            keyEvent.preventDefault();
            break;
          case 'Escape':
            onClose();
            markerElement?.focus();
            keyEvent.preventDefault();
        }
      },
    });
    return () => sub.unsubscribe();
  });
  const grafanaDS = useDatasource('-- Grafana --');

  const onClickAddCSV = () => {
    if (!grafanaDS) {
      return;
    }

    onChange(grafanaDS, [defaultFileUploadQuery]);
  };

  const currentDataSourceInstanceSettings = useDatasource(current);

  const popper = usePopper(markerElement, selectorElement, {
    placement: 'bottom-start',
    modifiers: [
      {
        name: 'offset',
        options: {
          offset: [0, 4],
        },
      },
      maxSize,
      applyMaxSize,
    ],
  });

  const onClose = useCallback(() => {
    setFilterTerm('');
    setOpen(false);
  }, [setOpen]);

  const ref = useRef<HTMLDivElement>(null);
  const { overlayProps, underlayProps } = useOverlay(
    {
      onClose: onClose,
      isDismissable: true,
      isOpen,
      shouldCloseOnInteractOutside: (element) => {
        return markerElement ? !markerElement.isSameNode(element) : false;
      },
    },
    ref
  );
  const { dialogProps } = useDialog({}, ref);

  const styles = useStyles2(getStylesDropdown);

  return (
    <div className={styles.container} data-testid={selectors.components.DataSourcePicker.container} style={{ width }}>
      {/* This clickable div is just extending the clickable area on the input element to include the prefix and suffix. */}
      {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions */}
      <div className={styles.trigger} onClick={openDropdown}>
        <Input
          className={inputHasFocus ? undefined : styles.input}
          data-testid={selectors.components.DataSourcePicker.inputV2}
          prefix={
            filterTerm && isOpen ? (
              <DataSourceLogoPlaceHolder />
            ) : (
              <DataSourceLogo dataSource={currentDataSourceInstanceSettings} />
            )
          }
          suffix={<Icon name={isOpen ? 'search' : 'angle-down'} />}
          placeholder={hideTextValue ? '' : dataSourceLabel(currentDataSourceInstanceSettings)}
          onClick={openDropdown}
          onFocus={() => {
            setInputHasFocus(true);
          }}
          onBlur={() => {
            setInputHasFocus(false);
            onClose();
          }}
          onKeyDown={onKeyDown}
          value={filterTerm}
          onChange={(e) => {
            openDropdown();
            setFilterTerm(e.currentTarget.value);
          }}
          ref={setMarkerElement}
        ></Input>
      </div>
      {isOpen ? (
        <Portal>
          <div {...underlayProps} />
          <div
            ref={ref}
            {...overlayProps}
            {...dialogProps}
            onMouseDown={(e) => {
              e.preventDefault(); /** Need to prevent default here to stop onMouseDown to trigger onBlur of the input element */
            }}
          >
            <PickerContent
              keyboardEvents={keyboardEvents}
              filterTerm={filterTerm}
              onChange={(
                ds: DataSourceInstanceSettings<DataSourceJsonData>,
                defaultQueries?: DataQuery[] | GrafanaQuery[]
              ) => {
                onClose();
                onChange(ds, defaultQueries);
              }}
              onClose={onClose}
              current={currentDataSourceInstanceSettings}
              style={popper.styles.popper}
              ref={setSelectorElement}
              onClickAddCSV={onClickAddCSV}
              {...restProps}
              onDismiss={onClose}
              {...popper.attributes.popper}
            />
          </div>
        </Portal>
      ) : null}
    </div>
  );
}

function getStylesDropdown(theme: GrafanaTheme2) {
  return {
    container: css`
      position: relative;
    `,
    trigger: css`
      cursor: pointer;
    `,
    input: css`
      input {
        cursor: pointer;
      }
      input::placeholder {
        color: ${theme.colors.text.primary};
      }
    `,
  };
}

const PickerContent = React.forwardRef<HTMLDivElement, PickerContentProps>((props, ref) => {
  const { filterTerm, onChange, onClose, onClickAddCSV, current } = props;
  const changeCallback = useCallback(
    (ds: DataSourceInstanceSettings<DataSourceJsonData>) => {
      onChange(ds);
      reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.SELECT_DS, ds_type: ds.type });
    },
    [onChange]
  );

  const clickAddCSVCallback = useCallback(() => {
    onClickAddCSV?.();
    onClose();
    reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.ADD_FILE });
  }, [onClickAddCSV, onClose]);

  const styles = useStyles2(getStylesPickerContent);

  return (
    <div style={props.style} ref={ref} className={styles.container}>
      <CustomScrollbar>
        <DataSourceList
          {...props}
          enableKeyboardNavigation
          className={styles.dataSourceList}
          current={current}
          onChange={changeCallback}
          filter={(ds) => matchDataSourceWithSearch(ds, filterTerm)}
          onClickEmptyStateCTA={() =>
            reportInteraction(INTERACTION_EVENT_NAME, {
              item: INTERACTION_ITEM.CONFIG_NEW_DS_EMPTY_STATE,
            })
          }
        ></DataSourceList>
      </CustomScrollbar>
      <div className={styles.footer}>
        <ModalsController>
          {({ showModal, hideModal }) => (
            <Button
              size="sm"
              variant="secondary"
              fill="text"
              onClick={() => {
                onClose();
                showModal(DataSourceModal, {
                  reportedInteractionFrom: 'ds_picker',
                  dashboard: props.dashboard,
                  mixed: props.mixed,
                  current,
                  onDismiss: hideModal,
                  onChange: (ds, defaultQueries) => {
                    onChange(ds, defaultQueries);
                    hideModal();
                  },
                });
                reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.OPEN_ADVANCED_DS_PICKER });
              }}
            >
              Open advanced data source picker
              <Icon name="arrow-right" />
            </Button>
          )}
        </ModalsController>
        {onClickAddCSV && config.featureToggles.editPanelCSVDragAndDrop && (
          <Button variant="secondary" size="sm" onClick={clickAddCSVCallback}>
            Add csv or spreadsheet
          </Button>
        )}
      </div>
    </div>
  );
});
PickerContent.displayName = 'PickerContent';

function getStylesPickerContent(theme: GrafanaTheme2) {
  return {
    container: css`
      display: flex;
      flex-direction: column;
      max-width: 480px;
      background: ${theme.colors.background.primary};
      box-shadow: ${theme.shadows.z3};
    `,
    picker: css`
      background: ${theme.colors.background.secondary};
    `,
    dataSourceList: css`
      flex: 1;
    `,
    footer: css`
      flex: 0;
      display: flex;
      flex-direction: row-reverse;
      justify-content: space-between;
      padding: ${theme.spacing(1.5)};
      border-top: 1px solid ${theme.colors.border.weak};
      background-color: ${theme.colors.background.secondary};
    `,
  };
}
