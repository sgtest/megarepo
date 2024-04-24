import { locationService, setDataSourceSrv } from '@grafana/runtime';
import { AdHocFiltersVariable, sceneGraph } from '@grafana/scenes';

import { MockDataSourceSrv, mockDataSource } from '../alerting/unified/mocks';
import { DataSourceType } from '../alerting/unified/utils/datasource';
import { activateFullSceneTree } from '../dashboard-scene/utils/test-utils';

import { DataTrail } from './DataTrail';
import { MetricScene } from './MetricScene';
import { MetricSelectScene } from './MetricSelectScene';
import { MetricSelectedEvent, VAR_FILTERS } from './shared';

describe('DataTrail', () => {
  beforeAll(() => {
    setDataSourceSrv(
      new MockDataSourceSrv({
        prom: mockDataSource({
          name: 'Prometheus',
          type: DataSourceType.Prometheus,
        }),
      })
    );
  });

  describe('Given starting non-embedded trail with url sync and no url state', () => {
    let trail: DataTrail;
    const preTrailUrl = '/';

    beforeEach(() => {
      trail = new DataTrail({});
      locationService.push(preTrailUrl);
      activateFullSceneTree(trail);
    });

    it('Should default to metric select scene', () => {
      expect(trail.state.topScene).toBeInstanceOf(MetricSelectScene);
    });

    it('Should set history current step to 0', () => {
      expect(trail.state.history.state.currentStep).toBe(0);
    });

    it('Should set history step 0 parentIndex to -1', () => {
      expect(trail.state.history.state.steps[0].parentIndex).toBe(-1);
    });

    describe('And metric is selected', () => {
      beforeEach(() => {
        trail.publishEvent(new MetricSelectedEvent('metric_bucket'));
      });

      it('should switch scene to MetricScene', () => {
        expect(trail.state.metric).toBe('metric_bucket');
        expect(trail.state.topScene).toBeInstanceOf(MetricScene);
      });

      it('should sync state with url', () => {
        expect(locationService.getSearchObject().metric).toBe('metric_bucket');
      });

      it('should add history step', () => {
        expect(trail.state.history.state.steps[1].type).toBe('metric');
      });

      it('Should set history currentStep to 1', () => {
        expect(trail.state.history.state.currentStep).toBe(1);
      });

      it('Should set history step 1 parentIndex to 0', () => {
        expect(trail.state.history.state.steps[1].parentIndex).toBe(0);
      });

      it('Should have time range `from` be default "now-6h"', () => {
        expect(trail.state.$timeRange?.state.from).toBe('now-6h');
      });

      describe('And browser back button is pressed', () => {
        locationService.getHistory().goBack();

        it('Should return to original URL', () => {
          const { pathname } = locationService.getLocation();
          expect(pathname).toEqual(preTrailUrl);
        });
      });

      describe('And when changing the time range `from` to "now-1h"', () => {
        beforeEach(() => {
          trail.state.$timeRange?.setState({ from: 'now-1h' });
        });

        it('should sync state with url', () => {
          expect(locationService.getSearchObject().from).toBe('now-1h');
        });

        it('should add history step', () => {
          expect(trail.state.history.state.steps[2].type).toBe('time');
        });

        it('Should set history currentStep to 2', () => {
          expect(trail.state.history.state.currentStep).toBe(2);
        });

        it('Should set history step 2 parentIndex to 1', () => {
          expect(trail.state.history.state.steps[2].parentIndex).toBe(1);
        });

        it('Should have time range `from` be updated "now-1h"', () => {
          expect(trail.state.$timeRange?.state.from).toBe('now-1h');
        });

        it('Previous history step should have previous default `from` of "now-6h"', () => {
          expect(trail.state.history.state.steps[1].trailState.$timeRange?.state.from).toBe('now-6h');
        });

        it('Current history step should have new `from` of "now-1h"', () => {
          expect(trail.state.history.state.steps[2].trailState.$timeRange?.state.from).toBe('now-1h');
        });

        describe('And when traversing back to step 1', () => {
          beforeEach(() => {
            trail.state.history.goBackToStep(1);
          });

          it('Should set history currentStep to 1', () => {
            expect(trail.state.history.state.currentStep).toBe(1);
          });

          it('should sync state with url', () => {
            expect(locationService.getSearchObject().from).toBe('now-6h');
          });

          it('Should have time range `from` be set back to "now-6h"', () => {
            expect(trail.state.$timeRange?.state.from).toBe('now-6h');
          });

          describe('And then when changing the time range `from` to "now-15m"', () => {
            beforeEach(() => {
              trail.state.$timeRange?.setState({ from: 'now-15m' });
            });

            it('should sync state with url', () => {
              expect(locationService.getSearchObject().from).toBe('now-15m');
            });

            it('should add history step', () => {
              expect(trail.state.history.state.steps[3].type).toBe('time');
            });

            it('Should set history currentStep to 3', () => {
              expect(trail.state.history.state.currentStep).toBe(3);
            });

            it('Should set history step 3 parentIndex to 1', () => {
              expect(trail.state.history.state.steps[3].parentIndex).toBe(1);
            });

            it('Should have time range `from` be updated "now-15m"', () => {
              expect(trail.state.$timeRange?.state.from).toBe('now-15m');
            });

            it('History step 1 (parent) should have previous default `from` of "now-6h"', () => {
              expect(trail.state.history.state.steps[1].trailState.$timeRange?.state.from).toBe('now-6h');
            });

            it('History step 2 should still have `from` of "now-1h"', () => {
              expect(trail.state.history.state.steps[2].trailState.$timeRange?.state.from).toBe('now-1h');
            });

            describe('And then when returning again to step 1', () => {
              beforeEach(() => {
                trail.state.history.goBackToStep(1);
              });

              it('Should set history currentStep to 1', () => {
                expect(trail.state.history.state.currentStep).toBe(1);
              });

              it('should sync state with url', () => {
                expect(locationService.getSearchObject().from).toBe('now-6h');
              });

              it('History step 1 (parent) should have previous default `from` of "now-6h"', () => {
                expect(trail.state.history.state.steps[1].trailState.$timeRange?.state.from).toBe('now-6h');
              });

              it('History step 2 should still have `from` of "now-1h"', () => {
                expect(trail.state.history.state.steps[2].trailState.$timeRange?.state.from).toBe('now-1h');
              });

              it('History step 3 should still have `from` of "now-15m"', () => {
                expect(trail.state.history.state.steps[3].trailState.$timeRange?.state.from).toBe('now-15m');
              });

              it('Should have time range `from` be set back to "now-6h"', () => {
                expect(trail.state.$timeRange?.state.from).toBe('now-6h');
              });
            });
          });
        });
      });

      function getFilterVar() {
        const variable = sceneGraph.lookupVariable(VAR_FILTERS, trail);
        if (variable instanceof AdHocFiltersVariable) {
          return variable;
        }
        throw new Error('getFilterVar failed');
      }

      function getStepFilterVar(step: number) {
        const variable = trail.state.history.state.steps[step].trailState.$variables?.getByName(VAR_FILTERS);
        if (variable instanceof AdHocFiltersVariable) {
          return variable;
        }
        throw new Error(`getStepFilterVar failed for step ${step}`);
      }

      it('Should have default empty filter', () => {
        expect(getFilterVar().state.filters.length).toBe(0);
      });

      describe('And when changing the filter to zone=a', () => {
        beforeEach(() => {
          getFilterVar().setState({ filters: [{ key: 'zone', operator: '=', value: 'a' }] });
        });

        it('should sync state with url', () => {
          expect(decodeURIComponent(locationService.getSearchObject()['var-filters']?.toString()!)).toBe('zone|=|a');
        });

        it('should add history step', () => {
          expect(trail.state.history.state.steps[2].type).toBe('filters');
        });

        it('Should set history currentStep to 2', () => {
          expect(trail.state.history.state.currentStep).toBe(2);
        });

        it('Should set history step 2 parentIndex to 1', () => {
          expect(trail.state.history.state.steps[2].parentIndex).toBe(1);
        });

        it('Should have filter be updated to "zone=a"', () => {
          expect(getFilterVar().state.filters[0].key).toBe('zone');
          expect(getFilterVar().state.filters[0].value).toBe('a');
        });

        it('Previous history step should have empty filter', () => {
          expect(getStepFilterVar(1).state.filters.length).toBe(0);
        });

        it('Current history step should have new filter zone=a', () => {
          expect(getStepFilterVar(2).state.filters[0].key).toBe('zone');
          expect(getStepFilterVar(2).state.filters[0].value).toBe('a');
        });

        describe('And when traversing back to step 1', () => {
          beforeEach(() => {
            trail.state.history.goBackToStep(1);
          });

          it('Should set history currentStep to 1', () => {
            expect(trail.state.history.state.currentStep).toBe(1);
          });

          it('should sync state with url', () => {
            expect(locationService.getSearchObject()['var-filters']).toBe('');
          });

          it('Should have filters set back to empty', () => {
            expect(getFilterVar().state.filters.length).toBe(0);
          });

          describe('And when changing the filter to zone=b', () => {
            beforeEach(() => {
              getFilterVar().setState({ filters: [{ key: 'zone', operator: '=', value: 'b' }] });
            });

            it('should sync state with url', () => {
              expect(decodeURIComponent(locationService.getSearchObject()['var-filters']?.toString()!)).toBe(
                'zone|=|b'
              );
            });

            it('should add history step', () => {
              expect(trail.state.history.state.steps[3].type).toBe('filters');
            });

            it('Should set history currentStep to 3', () => {
              expect(trail.state.history.state.currentStep).toBe(3);
            });

            it('Should set history step 3 parentIndex to 1', () => {
              expect(trail.state.history.state.steps[3].parentIndex).toBe(1);
            });

            it('Should have filter be updated to "zone=b"', () => {
              expect(getFilterVar().state.filters[0].key).toBe('zone');
              expect(getFilterVar().state.filters[0].value).toBe('b');
            });

            it('Parent history step 1 should still have empty filter', () => {
              expect(getStepFilterVar(1).state.filters.length).toBe(0);
            });

            it('History step 2 should still have old filter zone=a', () => {
              expect(getStepFilterVar(2).state.filters[0].key).toBe('zone');
              expect(getStepFilterVar(2).state.filters[0].value).toBe('a');
            });

            it('Current history step 3 should have new filter zone=b', () => {
              expect(getStepFilterVar(3).state.filters[0].key).toBe('zone');
              expect(getStepFilterVar(3).state.filters[0].value).toBe('b');
            });

            describe('And then when returning again to step 1', () => {
              beforeEach(() => {
                trail.state.history.goBackToStep(1);
              });

              it('Should set history currentStep to 1', () => {
                expect(trail.state.history.state.currentStep).toBe(1);
              });

              it('should sync state with url', () => {
                expect(locationService.getSearchObject()['var-filters']).toBe('');
              });

              it('Should have filters set back to empty', () => {
                expect(getFilterVar().state.filters.length).toBe(0);
              });

              it('History step 1 should still have empty filter', () => {
                expect(getStepFilterVar(1).state.filters.length).toBe(0);
              });

              it('History step 2 should still have old filter zone=a', () => {
                expect(getStepFilterVar(2).state.filters[0].key).toBe('zone');
                expect(getStepFilterVar(2).state.filters[0].value).toBe('a');
              });

              it('History step 3 should have new filter zone=b', () => {
                expect(getStepFilterVar(3).state.filters[0].key).toBe('zone');
                expect(getStepFilterVar(3).state.filters[0].value).toBe('b');
              });
            });
          });
        });
      });
    });

    describe('When going back to history step 1', () => {
      beforeEach(() => {
        trail.publishEvent(new MetricSelectedEvent('first_metric'));
        trail.publishEvent(new MetricSelectedEvent('second_metric'));
        trail.state.history.goBackToStep(1);
      });

      it('Should restore state and url', () => {
        expect(trail.state.metric).toBe('first_metric');
        expect(locationService.getSearchObject().metric).toBe('first_metric');
      });

      it('Should set history currentStep to 1', () => {
        expect(trail.state.history.state.currentStep).toBe(1);
      });

      it('Should not create another history step', () => {
        expect(trail.state.history.state.steps.length).toBe(3);
      });

      describe('But then selecting a new metric', () => {
        beforeEach(() => {
          trail.publishEvent(new MetricSelectedEvent('third_metric'));
        });

        it('Should create another history step', () => {
          expect(trail.state.history.state.steps.length).toBe(4);
        });

        it('Should set history current step to 3', () => {
          expect(trail.state.history.state.currentStep).toBe(3);
        });

        it('Should set history step 3 parent index to 1', () => {
          expect(trail.state.history.state.steps[3].parentIndex).toBe(1);
        });

        describe('And browser back button is pressed', () => {
          locationService.getHistory().goBack();

          it('Should return to original URL', () => {
            const { pathname } = locationService.getLocation();
            expect(pathname).toEqual(preTrailUrl);
          });
        });
      });
    });
    describe('When going back to history step 0', () => {
      beforeEach(() => {
        trail.publishEvent(new MetricSelectedEvent('first_metric'));
        trail.publishEvent(new MetricSelectedEvent('second_metric'));
        trail.state.history.goBackToStep(0);
      });

      it('Should remove metric from state and url', () => {
        expect(trail.state.metric).toBe(undefined);

        expect(locationService.getSearchObject().metric).toBe(undefined);
        expect(locationService.getSearch().has('metric')).toBe(false);
      });
    });
  });
});
