package models

import (
	"errors"
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestAdminConfiguration_AsSHA256(t *testing.T) {
	tc := []struct {
		name       string
		ac         *AdminConfiguration
		ciphertext string
	}{
		{
			name:       "AsSHA256",
			ac:         &AdminConfiguration{Alertmanagers: []string{"http://localhost:9093"}},
			ciphertext: "3ec9db375a5ba12f7c7b704922cf4b8e21a31e30d85be2386803829f0ee24410",
		},
	}

	for _, tt := range tc {
		t.Run(tt.name, func(t *testing.T) {
			require.Equal(t, tt.ciphertext, tt.ac.AsSHA256())
		})
	}
}

func TestAdminConfiguration_Validate(t *testing.T) {
	tc := []struct {
		name string
		ac   *AdminConfiguration
		err  error
	}{
		{
			name: "should return the first error if any of the Alertmanagers URL is invalid",
			ac:   &AdminConfiguration{Alertmanagers: []string{"http://localhost:9093", "http://›∂-)Æÿ ñ"}},
			err:  fmt.Errorf("parse \"http://›∂-)Æÿ ñ\": invalid character \" \" in host name"),
		},
		{
			name: "should not return any errors if all URLs are valid",
			ac:   &AdminConfiguration{Alertmanagers: []string{"http://localhost:9093"}},
		},
	}

	for _, tt := range tc {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.ac.Validate()
			if tt.err != nil {
				require.EqualError(t, err, tt.err.Error())
				return
			}

			require.NoError(t, err)
		})
	}
}

func TestStringToAlertmanagersChoice(t *testing.T) {
	tests := []struct {
		name                string
		str                 string
		alertmanagersChoice AlertmanagersChoice
		err                 error
	}{
		{
			"all alertmanagers",
			"all",
			AllAlertmanagers,
			nil,
		},
		{
			"internal alertmanager",
			"internal",
			InternalAlertmanager,
			nil,
		},
		{
			"external alertmanagers",
			"external",
			ExternalAlertmanagers,
			nil,
		},
		{
			"empty string value",
			"",
			AllAlertmanagers,
			nil,
		},
		{
			"invalid string",
			"invalid",
			0,
			errors.New("invalid alertmanager choice"),
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(tt *testing.T) {
			amc, err := StringToAlertmanagersChoice(test.str)
			if test.err != nil {
				require.EqualError(tt, err, test.err.Error())
			} else {
				require.NoError(tt, err)
			}

			require.Equal(tt, amc, test.alertmanagersChoice)
		})
	}
}
