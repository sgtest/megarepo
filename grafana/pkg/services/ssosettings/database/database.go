package database

import (
	"context"
	"time"

	"github.com/google/uuid"

	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/ssosettings"
	"github.com/grafana/grafana/pkg/services/ssosettings/models"
)

type SSOSettingsStore struct {
	sqlStore db.DB
	log      log.Logger
}

var (
	// timeNow makes it possible to test usage of time
	timeNow = time.Now
)

func ProvideStore(sqlStore db.DB) *SSOSettingsStore {
	return &SSOSettingsStore{
		sqlStore: sqlStore,
		log:      log.New("ssosettings.store"),
	}
}

var _ ssosettings.Store = (*SSOSettingsStore)(nil)

func (s *SSOSettingsStore) Get(ctx context.Context, provider string) (*models.SSOSettings, error) {
	result := models.SSOSettingsDTO{Provider: provider}
	err := s.sqlStore.WithDbSession(ctx, func(sess *db.Session) error {
		var err error
		sess.Table("sso_setting")
		found, err := sess.Where("is_deleted = ?", s.sqlStore.GetDialect().BooleanStr(false)).Get(&result)

		if err != nil {
			return err
		}

		if !found {
			return ssosettings.ErrNotFound
		}

		return nil
	})

	if err != nil {
		return nil, err
	}

	dto, err := result.ToSSOSettings()
	if err != nil {
		return nil, err
	}

	return dto, nil
}

func (s *SSOSettingsStore) List(ctx context.Context) ([]*models.SSOSettings, error) {
	dtos := make([]*models.SSOSettingsDTO, 0)
	err := s.sqlStore.WithDbSession(ctx, func(sess *db.Session) error {
		sess.Table("sso_setting")
		err := sess.Where("is_deleted = ?", s.sqlStore.GetDialect().BooleanStr(false)).Find(&dtos)

		if err != nil {
			return err
		}

		return nil
	})

	if err != nil {
		return nil, err
	}

	settings := make([]*models.SSOSettings, 0)
	for _, dto := range dtos {
		item, err := dto.ToSSOSettings()
		if err != nil {
			s.log.Warn("Failed to convert DB settings to SSOSettings for provider " + dto.Provider)
			continue
		}

		settings = append(settings, item)
	}

	return settings, nil
}

func (s *SSOSettingsStore) Upsert(ctx context.Context, settings models.SSOSettings) error {
	dto, err := settings.ToSSOSettingsDTO()
	if err != nil {
		return err
	}

	return s.sqlStore.WithDbSession(ctx, func(sess *db.Session) error {
		existing := &models.SSOSettingsDTO{
			Provider:  dto.Provider,
			IsDeleted: false,
		}
		found, err := sess.UseBool("is_deleted").Exist(existing)
		if err != nil {
			return err
		}

		now := timeNow().UTC()

		if found {
			updated := &models.SSOSettingsDTO{
				Settings:  dto.Settings,
				Updated:   now,
				IsDeleted: false,
			}
			_, err = sess.UseBool("is_deleted").Update(updated, existing)
		} else {
			_, err = sess.Insert(&models.SSOSettingsDTO{
				ID:       uuid.New().String(),
				Provider: dto.Provider,
				Settings: dto.Settings,
				Created:  now,
				Updated:  now,
			})
		}

		return err
	})
}

func (s *SSOSettingsStore) Patch(ctx context.Context, provider string, data map[string]interface{}) error {
	panic("not implemented") // TODO: Implement
}

func (s *SSOSettingsStore) Delete(ctx context.Context, provider string) error {
	return s.sqlStore.WithDbSession(ctx, func(sess *db.Session) error {
		existing := &models.SSOSettingsDTO{
			Provider:  provider,
			IsDeleted: false,
		}

		found, err := sess.UseBool("is_deleted").Get(existing)
		if err != nil {
			return err
		}

		if !found {
			return ssosettings.ErrNotFound
		}

		existing.Updated = timeNow().UTC()
		existing.IsDeleted = true

		_, err = sess.ID(existing.ID).MustCols("updated", "is_deleted").Update(existing)
		return err
	})
}
