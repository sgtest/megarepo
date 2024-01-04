package spec

import (
	"time"

	"github.com/grafana/regexp"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var codeClassPattern = regexp.MustCompile(`\dx+`)

type MonitoringSpec struct {
	// Alerts is a list of alert configurations for the deployment
	Alerts MonitoringAlertsSpec `yaml:"alerts"`
}

func (s *MonitoringSpec) Validate() []error {
	if s == nil {
		return nil
	}
	var errs []error
	errs = append(errs, s.Alerts.Validate()...)
	return errs
}

type MonitoringAlertsSpec struct {
	ResponseCodeRatios []ResponseCodeRatioAlertSpec `yaml:"responseCodeRatios"`
}

type ResponseCodeRatioAlertSpec struct {
	ID           string   `yaml:"id"`
	Name         string   `yaml:"name"`
	Description  *string  `yaml:"description,omitempty"`
	Code         *int     `yaml:"code,omitempty"`
	CodeClass    *string  `yaml:"codeClass,omitempty"`
	ExcludeCodes []string `yaml:"excludeCodes,omitempty"`
	Duration     *string  `yaml:"duration,omitempty"`
	Ratio        float64  `yaml:"ratio"`
}

func (s *MonitoringAlertsSpec) Validate() []error {
	var errs []error
	// Use map to contain seen IDs to ensure uniqueness
	ids := make(map[string]struct{})
	for _, r := range s.ResponseCodeRatios {
		if r.ID == "" {
			errs = append(errs, errors.New("responseCodeRatios[].id is required and cannot be empty"))
		}
		if _, ok := ids[r.ID]; ok {
			errs = append(errs, errors.Newf("response code alert IDs must be unique, found duplicate ID: %s", r.ID))
		}
		ids[r.ID] = struct{}{}
		errs = append(errs, r.Validate()...)
	}
	return errs
}

func (r *ResponseCodeRatioAlertSpec) Validate() []error {
	var errs []error

	if r.ID == "" {
		errs = append(errs, errors.New("responseCodeRatios[].id is required"))
	}

	if r.Name == "" {
		errs = append(errs, errors.New("responseCodeRatios[].name is required"))
	}

	if r.Ratio < 0 || r.Ratio > 1 {
		errs = append(errs, errors.New("responseCodeRatios[].ratio must be between 0 and 1"))
	}

	if r.CodeClass != nil && r.Code != nil {
		errs = append(errs, errors.New("only one of responseCodeRatios[].code or responseCodeRatios[].codeClass should be specified"))
	}

	if r.Code != nil && *r.Code <= 0 {
		errs = append(errs, errors.New("responseCodeRatios[].code must be positive"))
	}

	if r.CodeClass != nil {
		if !codeClassPattern.MatchString(*r.CodeClass) {
			errs = append(errs, errors.New("responseCodeRatios[].codeClass must match the format Nxx (e.g. 4xx, 5xx)"))
		}
	}

	if r.Duration != nil {
		duration, err := time.ParseDuration(*r.Duration)
		if err != nil {
			errs = append(errs, errors.Wrap(err, "responseCodeRatios[].duration must be in the format of XXs"))
		} else if duration%time.Minute != 0 {
			errs = append(errs, errors.New("responseCodeRatios[].duration must be a multiple of 60s"))
		}
	}

	return errs
}
