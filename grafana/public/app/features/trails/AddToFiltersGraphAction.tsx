import React from 'react';

import { DataFrame } from '@grafana/data';
import {
  SceneObjectState,
  SceneObjectBase,
  SceneComponentProps,
  sceneGraph,
  AdHocFiltersVariable,
} from '@grafana/scenes';
import { Button } from '@grafana/ui';

import { getMetricSceneFor } from './utils';

export interface AddToFiltersGraphActionState extends SceneObjectState {
  frame: DataFrame;
}

export class AddToFiltersGraphAction extends SceneObjectBase<AddToFiltersGraphActionState> {
  public onClick = () => {
    const variable = sceneGraph.lookupVariable('filters', this);
    if (!(variable instanceof AdHocFiltersVariable)) {
      return;
    }

    const labels = this.state.frame.fields[1]?.labels ?? {};
    if (Object.keys(labels).length !== 1) {
      return;
    }

    // close action view
    const metricScene = getMetricSceneFor(this);
    metricScene.setActionView(undefined);

    const labelName = Object.keys(labels)[0];

    variable.state.set.setState({
      filters: [
        ...variable.state.set.state.filters,
        {
          key: labelName,
          operator: '=',
          value: labels[labelName],
        },
      ],
    });
  };

  public static Component = ({ model }: SceneComponentProps<AddToFiltersGraphAction>) => {
    return (
      <Button variant="primary" size="sm" fill="text" onClick={model.onClick}>
        Add to filters
      </Button>
    );
  };
}
