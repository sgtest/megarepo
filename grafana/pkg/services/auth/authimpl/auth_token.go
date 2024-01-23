package authimpl

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net"
	"strings"
	"time"

	"golang.org/x/sync/singleflight"

	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/serverlock"
	"github.com/grafana/grafana/pkg/models/usertoken"
	"github.com/grafana/grafana/pkg/services/auth"
	"github.com/grafana/grafana/pkg/services/quota"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/util"
)

var (
	getTime            = time.Now
	errTokenNotRotated = errors.New("token was not rotated")
	errUserIDInvalid   = errors.New("invalid user ID")
)

func ProvideUserAuthTokenService(sqlStore db.DB,
	serverLockService *serverlock.ServerLockService,
	quotaService quota.Service,
	cfg *setting.Cfg) (*UserAuthTokenService, error) {
	s := &UserAuthTokenService{
		sqlStore:          sqlStore,
		serverLockService: serverLockService,
		cfg:               cfg,
		log:               log.New("auth"),
		singleflight:      new(singleflight.Group),
	}

	defaultLimits, err := readQuotaConfig(cfg)
	if err != nil {
		return s, err
	}

	if err := quotaService.RegisterQuotaReporter(&quota.NewUsageReporter{
		TargetSrv:     auth.QuotaTargetSrv,
		DefaultLimits: defaultLimits,
		Reporter:      s.reportActiveTokenCount,
	}); err != nil {
		return s, err
	}

	return s, nil
}

type UserAuthTokenService struct {
	sqlStore          db.DB
	serverLockService *serverlock.ServerLockService
	cfg               *setting.Cfg
	log               log.Logger
	singleflight      *singleflight.Group
}

func (s *UserAuthTokenService) CreateToken(ctx context.Context, user *user.User, clientIP net.IP, userAgent string) (*auth.UserToken, error) {
	token, hashedToken, err := generateAndHashToken(s.cfg.SecretKey)
	if err != nil {
		return nil, err
	}

	now := getTime().Unix()
	clientIPStr := clientIP.String()
	if len(clientIP) == 0 {
		clientIPStr = ""
	}

	userAuthToken := userAuthToken{
		UserId:        user.ID,
		AuthToken:     hashedToken,
		PrevAuthToken: hashedToken,
		ClientIp:      clientIPStr,
		UserAgent:     userAgent,
		RotatedAt:     now,
		CreatedAt:     now,
		UpdatedAt:     now,
		SeenAt:        0,
		RevokedAt:     0,
		AuthTokenSeen: false,
	}

	err = s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		_, err = dbSession.Insert(&userAuthToken)
		return err
	})

	if err != nil {
		return nil, err
	}

	userAuthToken.UnhashedToken = token

	ctxLogger := s.log.FromContext(ctx)
	ctxLogger.Debug("User auth token created", "tokenID", userAuthToken.Id, "userID", userAuthToken.UserId, "clientIP", userAuthToken.ClientIp, "userAgent", userAuthToken.UserAgent, "authToken", userAuthToken.AuthToken)

	var userToken auth.UserToken
	err = userAuthToken.toUserToken(&userToken)

	return &userToken, err
}

func (s *UserAuthTokenService) LookupToken(ctx context.Context, unhashedToken string) (*auth.UserToken, error) {
	hashedToken := hashToken(s.cfg.SecretKey, unhashedToken)
	var model userAuthToken
	var exists bool
	var err error
	err = s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		exists, err = dbSession.Where("(auth_token = ? OR prev_auth_token = ?)",
			hashedToken,
			hashedToken).
			Get(&model)

		return err
	})
	if err != nil {
		return nil, err
	}

	if !exists {
		return nil, auth.ErrUserTokenNotFound
	}

	ctxLogger := s.log.FromContext(ctx)

	if model.RevokedAt > 0 {
		ctxLogger.Debug("User token has been revoked", "userID", model.UserId, "tokenID", model.Id, "revokedAt", model.RevokedAt)
		return nil, &auth.TokenRevokedError{
			UserID:  model.UserId,
			TokenID: model.Id,
		}
	}

	if model.CreatedAt <= s.createdAfterParam() || model.RotatedAt <= s.rotatedAfterParam() {
		ctxLogger.Debug("User token has expired", "userID", model.UserId, "tokenID", model.Id, "createdAt", model.CreatedAt, "rotatedAt", model.RotatedAt)
		return nil, &auth.TokenExpiredError{
			UserID:  model.UserId,
			TokenID: model.Id,
		}
	}

	// Current incoming token is the previous auth token in the DB and the auth_token_seen is true
	if model.AuthToken != hashedToken && model.PrevAuthToken == hashedToken && model.AuthTokenSeen {
		model.AuthTokenSeen = false
		model.RotatedAt = getTime().Add(-usertoken.UrgentRotateTime).Unix()

		var affectedRows int64
		err = s.sqlStore.WithTransactionalDbSession(ctx, func(dbSession *db.Session) error {
			affectedRows, err = dbSession.Where("id = ? AND prev_auth_token = ? AND rotated_at < ?",
				model.Id,
				model.PrevAuthToken,
				model.RotatedAt).
				AllCols().Update(&model)

			return err
		})

		if err != nil {
			return nil, err
		}

		if affectedRows == 0 {
			ctxLogger.Debug("Prev seen token unchanged", "tokenID", model.Id, "userID", model.UserId, "clientIP", model.ClientIp, "userAgent", model.UserAgent, "authToken", model.AuthToken)
		} else {
			ctxLogger.Debug("Prev seen token", "tokenID", model.Id, "userID", model.UserId, "clientIP", model.ClientIp, "userAgent", model.UserAgent, "authToken", model.AuthToken)
		}
	}

	// Current incoming token is not seen and it is the latest valid auth token in the db
	if !model.AuthTokenSeen && model.AuthToken == hashedToken {
		model.AuthTokenSeen = true
		model.SeenAt = getTime().Unix()

		var affectedRows int64
		err = s.sqlStore.WithTransactionalDbSession(ctx, func(dbSession *db.Session) error {
			affectedRows, err = dbSession.Where("id = ? AND auth_token = ?",
				model.Id,
				model.AuthToken).
				AllCols().Update(&model)

			return err
		})

		if err != nil {
			return nil, err
		}

		if affectedRows == 0 {
			ctxLogger.Debug("Seen wrong token", "tokenID", model.Id, "userID", model.UserId, "clientIP", model.ClientIp, "userAgent", model.UserAgent, "authToken", model.AuthToken)
		} else {
			ctxLogger.Debug("Seen token", "tokenID", model.Id, "userID", model.UserId, "clientIP", model.ClientIp, "userAgent", model.UserAgent, "authToken", model.AuthToken)
		}
	}

	model.UnhashedToken = unhashedToken

	var userToken auth.UserToken
	err = model.toUserToken(&userToken)

	return &userToken, err
}

func (s *UserAuthTokenService) RotateToken(ctx context.Context, cmd auth.RotateCommand) (*auth.UserToken, error) {
	if cmd.UnHashedToken == "" {
		return nil, auth.ErrInvalidSessionToken
	}

	res, err, _ := s.singleflight.Do(cmd.UnHashedToken, func() (any, error) {
		token, err := s.LookupToken(ctx, cmd.UnHashedToken)
		if err != nil {
			return nil, err
		}
		s.log.FromContext(ctx).Debug("Rotating token", "tokenID", token.Id, "userID", token.UserId, "createdAt", token.CreatedAt, "rotatedAt", token.RotatedAt)

		newToken, err := s.rotateToken(ctx, token, cmd.IP, cmd.UserAgent)

		if errors.Is(err, errTokenNotRotated) {
			return token, nil
		}

		if err != nil {
			return nil, err
		}

		return newToken, nil
	})

	if err != nil {
		return nil, err
	}

	return res.(*auth.UserToken), nil
}

func (s *UserAuthTokenService) rotateToken(ctx context.Context, token *auth.UserToken, clientIP net.IP, userAgent string) (*auth.UserToken, error) {
	var clientIPStr string
	if clientIP != nil {
		clientIPStr = clientIP.String()
	}

	newToken, hashedToken, err := generateAndHashToken(s.cfg.SecretKey)
	if err != nil {
		return nil, err
	}

	sql := `
		UPDATE user_auth_token
		SET
			seen_at = 0,
			user_agent = ?,
			client_ip = ?,
			prev_auth_token = auth_token,
			auth_token = ?,
			auth_token_seen = ?,
			rotated_at = ?
		WHERE id = ?
	`

	now := getTime()
	var affected int64
	err = s.sqlStore.WithTransactionalDbSession(ctx, func(dbSession *db.Session) error {
		res, err := dbSession.Exec(sql, userAgent, clientIPStr, hashedToken, s.sqlStore.GetDialect().BooleanStr(false), now.Unix(), token.Id)
		if err != nil {
			return err
		}

		affected, err = res.RowsAffected()
		return err
	})

	if err != nil {
		return nil, err
	}

	if affected < 1 {
		return nil, errTokenNotRotated
	}

	token.PrevAuthToken = token.AuthToken
	token.AuthToken = hashedToken
	token.UnhashedToken = newToken
	token.AuthTokenSeen = false
	token.RotatedAt = now.Unix()

	return token, nil
}

func (s *UserAuthTokenService) TryRotateToken(ctx context.Context, token *auth.UserToken,
	clientIP net.IP, userAgent string) (bool, *auth.UserToken, error) {
	if token == nil {
		return false, nil, nil
	}

	model, err := userAuthTokenFromUserToken(token)
	if err != nil {
		return false, nil, err
	}

	now := getTime()

	type rotationResult struct {
		rotated  bool
		newToken *auth.UserToken
	}

	rotResult, err, _ := s.singleflight.Do(fmt.Sprint(model.Id), func() (any, error) {
		var needsRotation bool
		rotatedAt := time.Unix(model.RotatedAt, 0)
		if model.AuthTokenSeen {
			needsRotation = rotatedAt.Before(now.Add(-time.Duration(s.cfg.TokenRotationIntervalMinutes) * time.Minute))
		} else {
			needsRotation = rotatedAt.Before(now.Add(-usertoken.UrgentRotateTime))
		}

		if !needsRotation {
			return &rotationResult{rotated: false}, nil
		}

		ctxLogger := s.log.FromContext(ctx)
		ctxLogger.Debug("Token needs rotation", "tokenID", model.Id, "authTokenSeen", model.AuthTokenSeen, "rotatedAt", rotatedAt)

		clientIPStr := clientIP.String()
		if len(clientIP) == 0 {
			clientIPStr = ""
		}
		newToken, err := util.RandomHex(16)
		if err != nil {
			return nil, err
		}
		hashedToken := hashToken(s.cfg.SecretKey, newToken)

		// very important that auth_token_seen is set after the prev_auth_token = case when ... for mysql to function correctly
		sql := `
			UPDATE user_auth_token
			SET
				seen_at = 0,
				user_agent = ?,
				client_ip = ?,
				prev_auth_token = case when auth_token_seen = ? then auth_token else prev_auth_token end,
				auth_token = ?,
				auth_token_seen = ?,
				rotated_at = ?
			WHERE id = ? AND (auth_token_seen = ? OR rotated_at < ?)`

		var affected int64
		err = s.sqlStore.WithTransactionalDbSession(ctx, func(dbSession *db.Session) error {
			res, err := dbSession.Exec(sql, userAgent, clientIPStr, s.sqlStore.GetDialect().BooleanStr(true), hashedToken,
				s.sqlStore.GetDialect().BooleanStr(false), now.Unix(), model.Id, s.sqlStore.GetDialect().BooleanStr(true),
				now.Add(-30*time.Second).Unix())
			if err != nil {
				return err
			}

			affected, err = res.RowsAffected()
			return err
		})

		if err != nil {
			return nil, err
		}

		if affected > 0 {
			ctxLogger.Debug("Auth token rotated", "affected", affected, "tokenID", model.Id, "userID", model.UserId)
			model.UnhashedToken = newToken
			var result auth.UserToken
			if err := model.toUserToken(&result); err != nil {
				return nil, err
			}
			return &rotationResult{
				rotated:  true,
				newToken: &result,
			}, nil
		}

		return &rotationResult{rotated: false}, nil
	})

	if err != nil {
		return false, nil, err
	}

	result := rotResult.(*rotationResult)

	return result.rotated, result.newToken, nil
}

func (s *UserAuthTokenService) RevokeToken(ctx context.Context, token *auth.UserToken, soft bool) error {
	if token == nil {
		return auth.ErrUserTokenNotFound
	}

	model, err := userAuthTokenFromUserToken(token)
	if err != nil {
		return err
	}

	var rowsAffected int64

	if soft {
		model.RevokedAt = getTime().Unix()
		err = s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
			rowsAffected, err = dbSession.ID(model.Id).Update(model)
			return err
		})
	} else {
		err = s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
			rowsAffected, err = dbSession.Delete(model)
			return err
		})
	}

	if err != nil {
		return err
	}

	ctxLogger := s.log.FromContext(ctx)

	if rowsAffected == 0 {
		ctxLogger.Debug("User auth token not found/revoked", "tokenID", model.Id, "userID", model.UserId, "clientIP", model.ClientIp, "userAgent", model.UserAgent)
		return auth.ErrUserTokenNotFound
	}

	ctxLogger.Debug("User auth token revoked", "tokenID", model.Id, "userID", model.UserId, "clientIP", model.ClientIp, "userAgent", model.UserAgent, "soft", soft)

	return nil
}

func (s *UserAuthTokenService) RevokeAllUserTokens(ctx context.Context, userId int64) error {
	return s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		sql := `DELETE from user_auth_token WHERE user_id = ?`
		res, err := dbSession.Exec(sql, userId)
		if err != nil {
			return err
		}

		affected, err := res.RowsAffected()
		if err != nil {
			return err
		}

		s.log.FromContext(ctx).Debug("All user tokens for user revoked", "userID", userId, "count", affected)

		return err
	})
}

func (s *UserAuthTokenService) BatchRevokeAllUserTokens(ctx context.Context, userIds []int64) error {
	return s.sqlStore.WithTransactionalDbSession(ctx, func(dbSession *db.Session) error {
		if len(userIds) == 0 {
			return nil
		}

		user_id_params := strings.Repeat(",?", len(userIds)-1)
		sql := "DELETE from user_auth_token WHERE user_id IN (?" + user_id_params + ")"

		params := []any{sql}
		for _, v := range userIds {
			params = append(params, v)
		}

		res, err := dbSession.Exec(params...)
		if err != nil {
			return err
		}

		affected, err := res.RowsAffected()
		if err != nil {
			return err
		}

		s.log.FromContext(ctx).Debug("All user tokens for given users revoked", "usersCount", len(userIds), "count", affected)

		return err
	})
}

func (s *UserAuthTokenService) GetUserToken(ctx context.Context, userId, userTokenId int64) (*auth.UserToken, error) {
	var result auth.UserToken
	err := s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		var token userAuthToken
		exists, err := dbSession.Where("id = ? AND user_id = ?", userTokenId, userId).Get(&token)
		if err != nil {
			return err
		}

		if !exists {
			return auth.ErrUserTokenNotFound
		}

		return token.toUserToken(&result)
	})

	return &result, err
}

func (s *UserAuthTokenService) GetUserTokens(ctx context.Context, userId int64) ([]*auth.UserToken, error) {
	result := []*auth.UserToken{}
	err := s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		var tokens []*userAuthToken
		err := dbSession.Where("user_id = ? AND created_at > ? AND rotated_at > ? AND revoked_at = 0",
			userId,
			s.createdAfterParam(),
			s.rotatedAfterParam()).
			Find(&tokens)
		if err != nil {
			return err
		}

		for _, token := range tokens {
			var userToken auth.UserToken
			if err := token.toUserToken(&userToken); err != nil {
				return err
			}
			result = append(result, &userToken)
		}

		return nil
	})

	return result, err
}

// ActiveTokenCount returns the number of active tokens. If userID is nil, the count is for all users.
func (s *UserAuthTokenService) ActiveTokenCount(ctx context.Context, userID *int64) (int64, error) {
	if userID != nil && *userID < 1 {
		return 0, errUserIDInvalid
	}

	var count int64
	err := s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		query := `SELECT COUNT(*) FROM user_auth_token WHERE created_at > ? AND rotated_at > ? AND revoked_at = 0`
		args := []interface{}{s.createdAfterParam(), s.rotatedAfterParam()}
		if userID != nil {
			query += " AND user_id = ?"
			args = append(args, *userID)
		}
		_, err := dbSession.SQL(query, args...).Get(&count)
		return err
	})

	return count, err
}

func (s *UserAuthTokenService) DeleteUserRevokedTokens(ctx context.Context, userID int64, window time.Duration) error {
	return s.sqlStore.WithDbSession(ctx, func(sess *db.Session) error {
		query := "DELETE FROM user_auth_token WHERE user_id = ? AND revoked_at > 0 AND revoked_at <= ?"
		res, err := sess.Exec(query, userID, time.Now().Add(-window).Unix())
		if err != nil {
			return err
		}

		rows, err := res.RowsAffected()
		if err != nil {
			return err
		}

		s.log.FromContext(ctx).Debug("Deleted user revoked tokens", "userID", userID, "count", rows)
		return err
	})
}

func (s *UserAuthTokenService) GetUserRevokedTokens(ctx context.Context, userId int64) ([]*auth.UserToken, error) {
	result := []*auth.UserToken{}
	err := s.sqlStore.WithDbSession(ctx, func(dbSession *db.Session) error {
		var tokens []*userAuthToken
		err := dbSession.Where("user_id = ? AND revoked_at > 0", userId).Asc("seen_at").Find(&tokens)
		if err != nil {
			return err
		}

		for _, token := range tokens {
			var userToken auth.UserToken
			if err := token.toUserToken(&userToken); err != nil {
				return err
			}
			result = append(result, &userToken)
		}

		return nil
	})

	return result, err
}

func (s *UserAuthTokenService) reportActiveTokenCount(ctx context.Context, _ *quota.ScopeParameters) (*quota.Map, error) {
	count, err := s.ActiveTokenCount(ctx, nil)
	if err != nil {
		return nil, err
	}

	tag, err := quota.NewTag(auth.QuotaTargetSrv, auth.QuotaTarget, quota.GlobalScope)
	if err != nil {
		return nil, err
	}

	u := &quota.Map{}
	u.Set(tag, count)

	return u, err
}

func (s *UserAuthTokenService) createdAfterParam() int64 {
	return getTime().Add(-s.cfg.LoginMaxLifetime).Unix()
}

func (s *UserAuthTokenService) rotatedAfterParam() int64 {
	return getTime().Add(-s.cfg.LoginMaxInactiveLifetime).Unix()
}

func createToken() (string, error) {
	token, err := util.RandomHex(16)
	if err != nil {
		return "", err
	}

	return token, nil
}

func hashToken(secretKey string, token string) string {
	hashBytes := sha256.Sum256([]byte(token + secretKey))
	return hex.EncodeToString(hashBytes[:])
}

func generateAndHashToken(secretKey string) (string, string, error) {
	token, err := createToken()
	if err != nil {
		return "", "", err
	}

	return token, hashToken(secretKey, token), nil
}

func readQuotaConfig(cfg *setting.Cfg) (*quota.Map, error) {
	limits := &quota.Map{}

	if cfg == nil {
		return limits, nil
	}

	globalQuotaTag, err := quota.NewTag(auth.QuotaTargetSrv, auth.QuotaTarget, quota.GlobalScope)
	if err != nil {
		return limits, err
	}

	limits.Set(globalQuotaTag, cfg.Quota.Global.Session)
	return limits, nil
}
