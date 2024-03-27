import React from 'react';
import { useLocation } from 'react-router-dom';

import { config } from '@grafana/runtime';
import { EmptyState, Grid } from '@grafana/ui';

import { CatalogPlugin, PluginListDisplayMode } from '../types';

import { PluginListItem } from './PluginListItem';

interface Props {
  plugins: CatalogPlugin[];
  displayMode: PluginListDisplayMode;
  isLoading?: boolean;
}

export const PluginList = ({ plugins, displayMode, isLoading }: Props) => {
  const isList = displayMode === PluginListDisplayMode.List;
  const { pathname } = useLocation();
  const pathName = config.appSubUrl + (pathname.endsWith('/') ? pathname.slice(0, -1) : pathname);

  if (!isLoading && plugins.length === 0) {
    return <EmptyState variant="not-found" />;
  }

  return (
    <Grid gap={3} {...(isList ? { columns: 1 } : { minColumnWidth: 34 })} data-testid="plugin-list">
      {isLoading
        ? new Array(50).fill(null).map((_, index) => <PluginListItem.Skeleton key={index} displayMode={displayMode} />)
        : plugins.map((plugin) => (
            <PluginListItem key={plugin.id} plugin={plugin} pathName={pathName} displayMode={displayMode} />
          ))}
    </Grid>
  );
};
