package schedule

import (
	"bytes"
	context "context"
	"fmt"
	"math/rand"
	"sync"
	"testing"
	"time"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	models "github.com/grafana/grafana/pkg/services/ngalert/models"
	"github.com/grafana/grafana/pkg/services/ngalert/writer"
	"github.com/grafana/grafana/pkg/util"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/testutil"
	"github.com/stretchr/testify/require"
)

func TestRecordingRule(t *testing.T) {
	gen := models.RuleGen.With(models.RuleGen.WithAllRecordingRules())
	// evalRetval carries the return value of Rule.Eval() calls.
	type evalRetval struct {
		success     bool
		droppedEval *Evaluation
	}

	t.Run("when rule evaluation is not stopped", func(t *testing.T) {
		t.Run("eval should send to evalCh", func(t *testing.T) {
			r := blankRecordingRuleForTests(context.Background())
			expected := time.Now()
			resultCh := make(chan evalRetval)
			data := &Evaluation{
				scheduledAt: expected,
				rule:        gen.GenerateRef(),
				folderTitle: util.GenerateShortUID(),
			}

			go func() {
				result, dropped := r.Eval(data)
				resultCh <- evalRetval{result, dropped}
			}()

			select {
			case ctx := <-r.evalCh:
				require.Equal(t, data, ctx)
				result := <-resultCh // blocks
				require.True(t, result.success)
				require.Nilf(t, result.droppedEval, "expected no dropped evaluations but got one")
			case <-time.After(5 * time.Second):
				t.Fatal("No message was received on eval channel")
			}
		})
	})

	t.Run("when rule evaluation is stopped", func(t *testing.T) {
		t.Run("eval should do nothing", func(t *testing.T) {
			r := blankRecordingRuleForTests(context.Background())
			r.Stop(nil)
			ev := &Evaluation{
				scheduledAt: time.Now(),
				rule:        gen.GenerateRef(),
				folderTitle: util.GenerateShortUID(),
			}

			success, dropped := r.Eval(ev)

			require.False(t, success)
			require.Nilf(t, dropped, "expected no dropped evaluations but got one")
		})

		t.Run("calling stop multiple times should not panic", func(t *testing.T) {
			r := blankRecordingRuleForTests(context.Background())
			r.Stop(nil)
			r.Stop(nil)
		})

		t.Run("stop should not panic if parent context stopped", func(t *testing.T) {
			ctx, cancelFn := context.WithCancel(context.Background())
			r := blankRecordingRuleForTests(ctx)
			cancelFn()
			r.Stop(nil)
		})
	})

	t.Run("eval should be thread-safe", func(t *testing.T) {
		r := blankRecordingRuleForTests(context.Background())
		wg := sync.WaitGroup{}
		go func() {
			for {
				select {
				case <-r.evalCh:
					time.Sleep(time.Microsecond)
				case <-r.ctx.Done():
					return
				}
			}
		}()

		for i := 0; i < 10; i++ {
			wg.Add(1)
			go func() {
				for i := 0; i < 20; i++ {
					max := 3
					if i <= 10 {
						max = 2
					}
					switch rand.Intn(max) + 1 {
					case 1:
						r.Update(RuleVersionAndPauseStatus{fingerprint(rand.Uint64()), false})
					case 2:
						r.Eval(&Evaluation{
							scheduledAt: time.Now(),
							rule:        gen.GenerateRef(),
							folderTitle: util.GenerateShortUID(),
						})
					case 3:
						r.Stop(nil)
					}
				}
				wg.Done()
			}()
		}

		wg.Wait()
	})

	t.Run("Run should exit if idle when Stop is called", func(t *testing.T) {
		rule := blankRecordingRuleForTests(context.Background())
		runResult := make(chan error)
		go func() {
			runResult <- rule.Run(models.AlertRuleKey{})
		}()

		rule.Stop(nil)

		select {
		case err := <-runResult:
			require.NoError(t, err)
		case <-time.After(5 * time.Second):
			t.Fatal("Run() never exited")
		}
	})
}

func blankRecordingRuleForTests(ctx context.Context) *recordingRule {
	ft := featuremgmt.WithFeatures(featuremgmt.FlagGrafanaManagedRecordingRules)
	return newRecordingRule(context.Background(), 0, nil, nil, ft, log.NewNopLogger(), nil, nil, writer.FakeWriter{})
}

func TestRecordingRule_Integration(t *testing.T) {
	gen := models.RuleGen.With(models.RuleGen.WithAllRecordingRules())
	ruleStore := newFakeRulesStore()
	reg := prometheus.NewPedanticRegistry()
	sch := setupScheduler(t, ruleStore, nil, reg, nil, nil)
	rule := gen.GenerateRef()
	ruleStore.PutRule(context.Background(), rule)
	folderTitle := ruleStore.getNamespaceTitle(rule.NamespaceUID)
	ruleFactory := ruleFactoryFromScheduler(sch)

	process := ruleFactory.new(context.Background(), rule)
	evalDoneChan := make(chan time.Time)
	process.(*recordingRule).evalAppliedHook = func(_ models.AlertRuleKey, t time.Time) {
		evalDoneChan <- t
	}
	now := time.Now()

	go func() {
		_ = process.Run(rule.GetKey())
	}()
	process.Eval(&Evaluation{
		scheduledAt: now,
		rule:        rule,
		folderTitle: folderTitle,
	})
	_ = waitForTimeChannel(t, evalDoneChan)

	t.Run("reports basic evaluation metrics", func(t *testing.T) {
		expectedMetric := fmt.Sprintf(
			`
			# HELP grafana_alerting_rule_evaluation_duration_seconds The time to evaluate a rule.
			# TYPE grafana_alerting_rule_evaluation_duration_seconds histogram
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="0.01"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="0.1"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="0.5"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="1"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="5"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="10"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="15"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="30"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="60"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="120"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="180"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="240"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="300"} 1
			grafana_alerting_rule_evaluation_duration_seconds_bucket{org="%[1]d",le="+Inf"} 1
			grafana_alerting_rule_evaluation_duration_seconds_sum{org="%[1]d"} 0
			grafana_alerting_rule_evaluation_duration_seconds_count{org="%[1]d"} 1
			# HELP grafana_alerting_rule_evaluations_total The total number of rule evaluations.
			# TYPE grafana_alerting_rule_evaluations_total counter
			grafana_alerting_rule_evaluations_total{org="%[1]d"} 1
			# HELP grafana_alerting_rule_evaluation_attempts_total The total number of rule evaluation attempts.
			 # TYPE grafana_alerting_rule_evaluation_attempts_total counter
			grafana_alerting_rule_evaluation_attempts_total{org="%[1]d"} 1
			`,
			rule.OrgID,
		)

		err := testutil.GatherAndCompare(reg, bytes.NewBufferString(expectedMetric),
			"grafana_alerting_rule_evaluation_duration_seconds",
			"grafana_alerting_rule_evaluations_total",
			"grafana_alerting_rule_evaluation_attempts_total",
		)
		require.NoError(t, err)
	})
}
