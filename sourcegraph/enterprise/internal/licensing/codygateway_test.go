package licensing

import (
	"testing"

	"github.com/google/go-cmp/cmp"
)

func TestNewCodyGatewayChatRateLimit(t *testing.T) {
	tests := []struct {
		name        string
		plan        Plan
		userCount   *int
		licenseTags []string
		want        CodyGatewayRateLimit
	}{
		{
			name:        "Enterprise plan with GPT tag and user count",
			plan:        PlanEnterprise1,
			userCount:   intPtr(50),
			licenseTags: []string{GPTLLMAccessTag},
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"openai/gpt-4", "openai/gpt-3.5-turbo"},
				Limit:           2500,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name:      "Enterprise plan with no GPT tag",
			plan:      PlanEnterprise1,
			userCount: intPtr(50),
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"anthropic/claude-v1", "anthropic/claude-instant-v1"},
				Limit:           2500,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name: "Enterprise plan with no user count",
			plan: PlanEnterprise1,
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"anthropic/claude-v1", "anthropic/claude-instant-v1"},
				Limit:           50,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name: "Non-enterprise plan with no GPT tag and no user count",
			plan: "unknown",
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"anthropic/claude-v1", "anthropic/claude-instant-v1"},
				Limit:           10,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := NewCodyGatewayChatRateLimit(tt.plan, tt.userCount, tt.licenseTags)
			if diff := cmp.Diff(got, tt.want); diff != "" {
				t.Fatalf("incorrect rate limit computed: %s", diff)
			}
		})
	}
}

func TestCodyGatewayCodeRateLimit(t *testing.T) {
	tests := []struct {
		name        string
		plan        Plan
		userCount   *int
		licenseTags []string
		want        CodyGatewayRateLimit
	}{
		{
			name:        "Enterprise plan with GPT tag and user count",
			plan:        PlanEnterprise1,
			userCount:   intPtr(50),
			licenseTags: []string{GPTLLMAccessTag},
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"openai/gpt-3.5-turbo"},
				Limit:           50000,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name:      "Enterprise plan with no GPT tag",
			plan:      PlanEnterprise1,
			userCount: intPtr(50),
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"anthropic/claude-instant-v1"},
				Limit:           50000,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name: "Enterprise plan with no user count",
			plan: PlanEnterprise1,
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"anthropic/claude-instant-v1"},
				Limit:           1000,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name: "Non-enterprise plan with no GPT tag and no user count",
			plan: "unknown",
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"anthropic/claude-instant-v1"},
				Limit:           100,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := NewCodyGatewayCodeRateLimit(tt.plan, tt.userCount, tt.licenseTags)
			if diff := cmp.Diff(got, tt.want); diff != "" {
				t.Fatalf("incorrect rate limit computed: %s", diff)
			}
		})
	}
}

func TestCodyGatewayEmbeddingsRateLimit(t *testing.T) {
	tests := []struct {
		name        string
		plan        Plan
		userCount   *int
		licenseTags []string
		want        CodyGatewayRateLimit
	}{
		{
			name:      "Enterprise plan",
			plan:      PlanEnterprise1,
			userCount: intPtr(50),
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"openai/text-embedding-ada-002"},
				Limit:           20 * 50 * 2_500_000 / 30,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name: "Enterprise plan with no user count",
			plan: PlanEnterprise1,
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"openai/text-embedding-ada-002"},
				Limit:           1 * 20 * 2_500_000 / 30,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
		{
			name: "Non-enterprise plan with no user count",
			plan: "unknown",
			want: CodyGatewayRateLimit{
				AllowedModels:   []string{"openai/text-embedding-ada-002"},
				Limit:           1 * 10 * 2_500_000 / 30,
				IntervalSeconds: 60 * 60 * 24,
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := NewCodyGatewayEmbeddingsRateLimit(tt.plan, tt.userCount, tt.licenseTags)
			if diff := cmp.Diff(got, tt.want); diff != "" {
				t.Fatalf("incorrect rate limit computed: %s", diff)
			}
		})
	}
}

func intPtr(i int) *int { return &i }
