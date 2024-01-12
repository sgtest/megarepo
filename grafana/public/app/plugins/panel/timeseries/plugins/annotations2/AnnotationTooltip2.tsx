import { css } from '@emotion/css';
import React, { useContext, useEffect } from 'react';

import { GrafanaTheme2, textUtil } from '@grafana/data';
import { HorizontalGroup, IconButton, LayoutItemContext, Tag, useStyles2 } from '@grafana/ui';
import alertDef from 'app/features/alerting/state/alertDef';

interface Props {
  annoVals: Record<string, any[]>;
  annoIdx: number;
  timeFormatter: (v: number) => string;
  canEdit: boolean;
  canDelete: boolean;
  onEdit: () => void;
  onDelete: () => void;
}

export const AnnotationTooltip2 = ({
  annoVals,
  annoIdx,
  timeFormatter,
  canEdit,
  canDelete,
  onEdit,
  onDelete,
}: Props) => {
  const styles = useStyles2(getStyles);

  const layoutCtx = useContext(LayoutItemContext);
  useEffect(() => layoutCtx.boostZIndex(), [layoutCtx]);

  let time = timeFormatter(annoVals.time[annoIdx]);
  let text = annoVals.text[annoIdx];

  if (annoVals.isRegion?.[annoIdx]) {
    time += ' - ' + timeFormatter(annoVals.timeEnd[annoIdx]);
  }

  let avatar;
  if (annoVals.login?.[annoIdx] && annoVals.avatarUrl?.[annoIdx]) {
    avatar = <img className={styles.avatar} alt="Annotation avatar" src={annoVals.avatarUrl[annoIdx]} />;
  }

  let state: React.ReactNode | null = null;
  let alertText = '';

  if (annoVals.alertId?.[annoIdx] !== undefined && annoVals.newState?.[annoIdx]) {
    const stateModel = alertDef.getStateDisplayModel(annoVals.newState[annoIdx]);
    state = (
      <div className={styles.alertState}>
        <i className={stateModel.stateClass}>{stateModel.text}</i>
      </div>
    );

    // alertText = alertDef.getAlertAnnotationInfo(annotation); // @TODO ??
  } else if (annoVals.title?.[annoIdx]) {
    text = annoVals.title[annoIdx] + '<br />' + (typeof text === 'string' ? text : '');
  }

  return (
    <div className={styles.wrapper}>
      <div className={styles.header}>
        <HorizontalGroup justify={'space-between'} align={'center'} spacing={'md'}>
          <div className={styles.meta}>
            <span>
              {avatar}
              {state}
            </span>
            {time}
          </div>
          {(canEdit || canDelete) && (
            <div className={styles.editControls}>
              {canEdit && <IconButton name={'pen'} size={'sm'} onClick={onEdit} tooltip="Edit" />}
              {canDelete && (
                <IconButton
                  name={'trash-alt'}
                  size={'sm'}
                  onClick={onDelete}
                  tooltip="Delete"
                  disabled={!annoVals.id?.[annoIdx]}
                />
              )}
            </div>
          )}
        </HorizontalGroup>
      </div>

      <div className={styles.body}>
        {text && <div className={styles.text} dangerouslySetInnerHTML={{ __html: textUtil.sanitize(text) }} />}
        {alertText}
        <div>
          <HorizontalGroup spacing="xs" wrap>
            {annoVals.tags?.[annoIdx]?.map((t: string, i: number) => <Tag name={t} key={`${t}-${i}`} />)}
          </HorizontalGroup>
        </div>
      </div>
    </div>
  );
};

const getStyles = (theme: GrafanaTheme2) => ({
  wrapper: css({
    zIndex: theme.zIndex.tooltip,
    whiteSpace: 'initial',
    borderRadius: theme.shape.radius.default,
    background: theme.colors.background.primary,
    border: `1px solid ${theme.colors.border.weak}`,
    boxShadow: theme.shadows.z2,
    userSelect: 'text',
  }),
  header: css({
    padding: theme.spacing(0.5, 1),
    borderBottom: `1px solid ${theme.colors.border.weak}`,
    fontWeight: theme.typography.fontWeightBold,
    fontSize: theme.typography.fontSize,
    color: theme.colors.text.primary,
    display: 'flex',
  }),
  meta: css({
    display: 'flex',
    justifyContent: 'space-between',
    color: theme.colors.text.primary,
    fontWeight: 400,
  }),
  editControls: css({
    display: 'flex',
    alignItems: 'center',
    '> :last-child': {
      marginLeft: 0,
    },
  }),
  body: css({
    padding: theme.spacing(1),
    fontSize: theme.typography.bodySmall.fontSize,
    color: theme.colors.text.secondary,
    fontWeight: 400,
    a: {
      color: theme.colors.text.link,
      '&:hover': {
        textDecoration: 'underline',
      },
    },
  }),
  text: css({
    paddingBottom: theme.spacing(1),
  }),
  avatar: css({
    borderRadius: theme.shape.radius.circle,
    width: 16,
    height: 16,
    marginRight: theme.spacing(1),
  }),
  alertState: css({
    paddingRight: theme.spacing(1),
    fontWeight: theme.typography.fontWeightMedium,
  }),
});
