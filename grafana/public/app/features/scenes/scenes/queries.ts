import { QueryRunnerState, SceneQueryRunner } from '@grafana/scenes';
import { TestData } from '@grafana-plugins/grafana-testdata-datasource/dataquery.gen';

export function getQueryRunnerWithRandomWalkQuery(
  overrides?: Partial<TestData>,
  queryRunnerOverrides?: Partial<QueryRunnerState>
) {
  return new SceneQueryRunner({
    queries: [
      {
        refId: 'A',
        datasource: {
          uid: 'gdev-testdata',
          type: 'testdata',
        },
        scenarioId: 'random_walk',
        ...overrides,
      },
    ],
    ...queryRunnerOverrides,
  });
}
