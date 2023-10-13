import { css } from '@emotion/css';
import React from 'react';

import {
  arrayUtils,
  DataFrame,
  Field,
  formattedValueToString,
  getFieldDisplayName,
  GrafanaTheme2,
  LinkModel,
} from '@grafana/data';
import { SortOrder, TooltipDisplayMode } from '@grafana/schema';
import { TextLink, useStyles2 } from '@grafana/ui';
import { renderValue } from 'app/plugins/panel/geomap/utils/uiUtils';

export interface Props {
  data?: DataFrame; // source data
  rowIndex?: number | null; // the hover row
  columnIndex?: number | null; // the hover column
  sortOrder?: SortOrder;
  mode?: TooltipDisplayMode | null;
  header?: string;
}

interface DisplayValue {
  name: string;
  value: unknown;
  valueString: string;
  highlight: boolean;
}

export const DataHoverView = ({ data, rowIndex, columnIndex, sortOrder, mode, header = undefined }: Props) => {
  const styles = useStyles2(getStyles);

  if (!data || rowIndex == null) {
    return null;
  }
  const fields = data.fields.map((f, idx) => {
    return { ...f, hovered: idx === columnIndex };
  });
  const visibleFields = fields.filter((f) => !Boolean(f.config.custom?.hideFrom?.tooltip));
  const traceIDField = visibleFields.find((field) => field.name === 'traceID') || fields[0];
  const orderedVisibleFields = [];
  // Only include traceID if it's visible and put it in front.
  if (visibleFields.filter((field) => traceIDField === field).length > 0) {
    orderedVisibleFields.push(traceIDField);
  }
  orderedVisibleFields.push(...visibleFields.filter((field) => traceIDField !== field));

  if (orderedVisibleFields.length === 0) {
    return null;
  }

  const displayValues: DisplayValue[] = [];
  const links: Array<LinkModel<Field>> = [];
  const linkLookup = new Set<string>();

  for (const field of orderedVisibleFields) {
    if (mode === TooltipDisplayMode.Single && columnIndex != null && !field.hovered) {
      continue;
    }

    const value = field.values[rowIndex];
    const fieldDisplay = field.display ? field.display(value) : { text: `${value}`, numeric: +value };

    if (field.getLinks) {
      field.getLinks({ calculatedValue: fieldDisplay, valueRowIndex: rowIndex }).forEach((link) => {
        const key = `${link.title}/${link.href}`;
        if (!linkLookup.has(key)) {
          links.push(link);
          linkLookup.add(key);
        }
      });
    }

    // Sanitize field by removing hovered property to fix unique display name issue
    const { hovered, ...sanitizedField } = field;

    displayValues.push({
      name: getFieldDisplayName(sanitizedField, data),
      value,
      valueString: formattedValueToString(fieldDisplay),
      highlight: field.hovered,
    });
  }

  if (sortOrder && sortOrder !== SortOrder.None) {
    displayValues.sort((a, b) => arrayUtils.sortValues(sortOrder)(a.value, b.value));
  }

  return (
    <div className={styles.wrapper}>
      {header && (
        <div className={styles.header}>
          <span className={styles.title}>{header}</span>
        </div>
      )}
      <table className={styles.infoWrap}>
        <tbody>
          {displayValues.map((displayValue, i) => (
            <tr key={`${i}/${rowIndex}`}>
              <th>{displayValue.name}</th>
              <td>{renderValue(displayValue.valueString)}</td>
            </tr>
          ))}
          {links.map((link, i) => (
            <tr key={i}>
              <th>Link</th>
              <td colSpan={2}>
                <TextLink href={link.href} external={link.target === '_blank'} weight={'medium'} inline={false}>
                  {link.title}
                </TextLink>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};
const getStyles = (theme: GrafanaTheme2) => {
  return {
    wrapper: css`
      background: ${theme.components.tooltip.background};
      border-radius: ${theme.shape.borderRadius(2)};
    `,
    header: css`
      background: ${theme.colors.background.secondary};
      align-items: center;
      align-content: center;
      display: flex;
      padding-bottom: ${theme.spacing(1)};
    `,
    title: css`
      font-weight: ${theme.typography.fontWeightMedium};
      overflow: hidden;
      display: inline-block;
      white-space: nowrap;
      text-overflow: ellipsis;
      flex-grow: 1;
    `,
    infoWrap: css`
      padding: ${theme.spacing(1)};
      background: transparent;
      border: none;
      th {
        font-weight: ${theme.typography.fontWeightMedium};
        padding: ${theme.spacing(0.25, 2, 0.25, 0)};
      }

      tr {
        border-bottom: 1px solid ${theme.colors.border.weak};
        &:last-child {
          border-bottom: none;
        }
      }
    `,
    highlight: css``,
    link: css`
      color: ${theme.colors.text.link};
    `,
  };
};
