package alerting

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/grafana/grafana/pkg/components/imguploader"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/metrics"
	"github.com/grafana/grafana/pkg/models"
	alertmodels "github.com/grafana/grafana/pkg/services/alerting/models"
	"github.com/grafana/grafana/pkg/services/notifications"
	"github.com/grafana/grafana/pkg/services/org"
	"github.com/grafana/grafana/pkg/services/rendering"
	"github.com/grafana/grafana/pkg/setting"
)

// for stubbing in tests
//
//nolint:gocritic
var newImageUploaderProvider = func(cfg *setting.Cfg) (imguploader.ImageUploader, error) {
	return imguploader.NewImageUploader(cfg)
}

// NotifierPlugin holds meta information about a notifier.
type NotifierPlugin struct {
	Type        string           `json:"type"`
	Name        string           `json:"name"`
	Heading     string           `json:"heading"`
	Description string           `json:"description"`
	Info        string           `json:"info"`
	Factory     NotifierFactory  `json:"-"`
	Options     []NotifierOption `json:"options"`
}

// NotifierOption holds information about options specific for the NotifierPlugin.
type NotifierOption struct {
	Element        ElementType    `json:"element"`
	InputType      InputType      `json:"inputType"`
	Label          string         `json:"label"`
	Description    string         `json:"description"`
	Placeholder    string         `json:"placeholder"`
	PropertyName   string         `json:"propertyName"`
	SelectOptions  []SelectOption `json:"selectOptions"`
	ShowWhen       ShowWhen       `json:"showWhen"`
	Required       bool           `json:"required"`
	ValidationRule string         `json:"validationRule"`
	Secure         bool           `json:"secure"`
	DependsOn      string         `json:"dependsOn"`
}

// InputType is the type of input that can be rendered in the frontend.
type InputType string

const (
	// InputTypeText will render a text field in the frontend
	InputTypeText = "text"
	// InputTypePassword will render a password field in the frontend
	InputTypePassword = "password"
)

// ElementType is the type of element that can be rendered in the frontend.
type ElementType string

const (
	// ElementTypeInput will render an input
	ElementTypeInput = "input"
	// ElementTypeSelect will render a select
	ElementTypeSelect = "select"
	// ElementTypeCheckbox will render a checkbox
	ElementTypeCheckbox = "checkbox"
	// ElementTypeTextArea will render a textarea
	ElementTypeTextArea = "textarea"
)

// SelectOption is a simple type for Options that have dropdown options. Should be used when Element is ElementTypeSelect.
type SelectOption struct {
	Value string `json:"value"`
	Label string `json:"label"`
}

// ShowWhen holds information about when options are dependant on other options.
type ShowWhen struct {
	Field string `json:"field"`
	Is    string `json:"is"`
}

func newNotificationService(cfg *setting.Cfg, renderService rendering.Service, sqlStore AlertStore, notificationSvc *notifications.NotificationService, decryptFn GetDecryptedValueFn) *notificationService {
	return &notificationService{
		cfg:                 cfg,
		log:                 log.New("alerting.notifier"),
		renderService:       renderService,
		sqlStore:            sqlStore,
		notificationService: notificationSvc,
		decryptFn:           decryptFn,
	}
}

type notificationService struct {
	cfg                 *setting.Cfg
	log                 log.Logger
	renderService       rendering.Service
	sqlStore            AlertStore
	notificationService *notifications.NotificationService
	decryptFn           GetDecryptedValueFn
}

func (n *notificationService) SendIfNeeded(evalCtx *EvalContext) error {
	notifierStates, err := n.getNeededNotifiers(evalCtx.Rule.OrgID, evalCtx.Rule.Notifications, evalCtx)
	if err != nil {
		n.log.Error("Failed to get alert notifiers", "error", err)
		return err
	}

	if len(notifierStates) == 0 {
		return nil
	}

	if notifierStates.ShouldUploadImage() {
		// Create a copy of EvalContext and give it a new, shorter, timeout context to upload the image
		uploadEvalCtx := *evalCtx
		timeout := n.cfg.AlertingNotificationTimeout / 2
		var uploadCtxCancel func()
		uploadEvalCtx.Ctx, uploadCtxCancel = context.WithTimeout(evalCtx.Ctx, timeout)

		// Try to upload the image without consuming all the time allocated for EvalContext
		if err = n.renderAndUploadImage(&uploadEvalCtx, timeout); err != nil {
			n.log.Error("Failed to render and upload alert panel image.", "ruleId", uploadEvalCtx.Rule.ID, "error", err)
		}
		uploadCtxCancel()
		evalCtx.ImageOnDiskPath = uploadEvalCtx.ImageOnDiskPath
		evalCtx.ImagePublicURL = uploadEvalCtx.ImagePublicURL
	}

	return n.sendNotifications(evalCtx, notifierStates)
}

func (n *notificationService) sendAndMarkAsComplete(evalContext *EvalContext, notifierState *notifierState) error {
	notifier := notifierState.notifier

	n.log.Debug("Sending notification", "type", notifier.GetType(), "uid", notifier.GetNotifierUID(), "isDefault", notifier.GetIsDefault())
	metrics.MAlertingNotificationSent.WithLabelValues(notifier.GetType()).Inc()

	if err := evalContext.evaluateNotificationTemplateFields(); err != nil {
		n.log.Error("Failed trying to evaluate notification template fields", "uid", notifier.GetNotifierUID(), "error", err)
	}

	if err := notifier.Notify(evalContext); err != nil {
		n.log.Error("Failed to send notification", "uid", notifier.GetNotifierUID(), "error", err)
		metrics.MAlertingNotificationFailed.WithLabelValues(notifier.GetType()).Inc()
		return err
	}

	if evalContext.IsTestRun {
		return nil
	}

	cmd := &alertmodels.SetAlertNotificationStateToCompleteCommand{
		ID:      notifierState.state.ID,
		Version: notifierState.state.Version,
	}

	return n.sqlStore.SetAlertNotificationStateToCompleteCommand(evalContext.Ctx, cmd)
}

func (n *notificationService) sendNotification(evalContext *EvalContext, notifierState *notifierState) error {
	if !evalContext.IsTestRun {
		setPendingCmd := &alertmodels.SetAlertNotificationStateToPendingCommand{
			ID:                           notifierState.state.ID,
			Version:                      notifierState.state.Version,
			AlertRuleStateUpdatedVersion: evalContext.Rule.StateChanges,
		}

		err := n.sqlStore.SetAlertNotificationStateToPendingCommand(evalContext.Ctx, setPendingCmd)
		if err != nil {
			if errors.Is(err, alertmodels.ErrAlertNotificationStateVersionConflict) {
				return nil
			}

			return err
		}

		// We need to update state version to be able to log
		// unexpected version conflicts when marking notifications as ok
		notifierState.state.Version = setPendingCmd.ResultVersion
	}

	return n.sendAndMarkAsComplete(evalContext, notifierState)
}

func (n *notificationService) sendNotifications(evalContext *EvalContext, notifierStates notifierStateSlice) error {
	for _, notifierState := range notifierStates {
		err := n.sendNotification(evalContext, notifierState)
		if err != nil {
			n.log.Error("Failed to send notification", "uid", notifierState.notifier.GetNotifierUID(), "error", err)
			if evalContext.IsTestRun {
				return err
			}
		}
	}
	return nil
}

func (n *notificationService) renderAndUploadImage(evalCtx *EvalContext, timeout time.Duration) (err error) {
	uploader, err := newImageUploaderProvider(n.cfg)
	if err != nil {
		return err
	}

	renderOpts := rendering.Opts{
		TimeoutOpts: rendering.TimeoutOpts{
			Timeout: timeout,
		},
		AuthOpts: rendering.AuthOpts{
			OrgID:   evalCtx.Rule.OrgID,
			OrgRole: org.RoleAdmin,
		},
		Width:           1000,
		Height:          500,
		ConcurrentLimit: n.cfg.AlertingRenderLimit,
		Theme:           models.ThemeDark,
	}

	ref, err := evalCtx.GetDashboardUID()
	if err != nil {
		return err
	}

	renderOpts.Path = fmt.Sprintf("d-solo/%s/%s?orgId=%d&panelId=%d", ref.UID, ref.Slug, evalCtx.Rule.OrgID, evalCtx.Rule.PanelID)

	n.log.Debug("Rendering alert panel image", "ruleId", evalCtx.Rule.ID, "urlPath", renderOpts.Path)
	start := time.Now()
	result, err := n.renderService.Render(evalCtx.Ctx, rendering.RenderPNG, renderOpts, nil)
	if err != nil {
		return err
	}
	took := time.Since(start)

	n.log.Debug("Rendered alert panel image", "ruleId", evalCtx.Rule.ID, "path", result.FilePath, "took", took)

	evalCtx.ImageOnDiskPath = result.FilePath

	n.log.Debug("Uploading alert panel image to external image store", "ruleId", evalCtx.Rule.ID, "path", evalCtx.ImageOnDiskPath)

	start = time.Now()
	evalCtx.ImagePublicURL, err = uploader.Upload(evalCtx.Ctx, evalCtx.ImageOnDiskPath)
	if err != nil {
		return err
	}
	took = time.Since(start)

	if evalCtx.ImagePublicURL != "" {
		n.log.Debug("Uploaded alert panel image to external image store", "ruleId", evalCtx.Rule.ID, "url", evalCtx.ImagePublicURL, "took", took)
	}

	return nil
}

func (n *notificationService) getNeededNotifiers(orgID int64, notificationUids []string, evalContext *EvalContext) (notifierStateSlice, error) {
	query := &alertmodels.GetAlertNotificationsWithUidToSendQuery{OrgID: orgID, UIDs: notificationUids}

	res, err := n.sqlStore.GetAlertNotificationsWithUidToSend(evalContext.Ctx, query)
	if err != nil {
		return nil, err
	}

	var result notifierStateSlice
	for _, notification := range res {
		not, err := InitNotifier(n.cfg, notification, n.decryptFn, n.notificationService)
		if err != nil {
			n.log.Error("Could not create notifier", "notifier", notification.UID, "error", err)
			continue
		}

		query := &alertmodels.GetOrCreateNotificationStateQuery{
			NotifierID: notification.ID,
			AlertID:    evalContext.Rule.ID,
			OrgID:      evalContext.Rule.OrgID,
		}

		state, err := n.sqlStore.GetOrCreateAlertNotificationState(evalContext.Ctx, query)
		if err != nil {
			n.log.Error("Could not get notification state.", "notifier", notification.ID, "error", err)
			continue
		}

		if not.ShouldNotify(evalContext.Ctx, evalContext, state) {
			result = append(result, &notifierState{
				notifier: not,
				state:    state,
			})
		}
	}

	return result, nil
}

// InitNotifier instantiate a new notifier based on the model.
func InitNotifier(cfg *setting.Cfg, model *alertmodels.AlertNotification, fn GetDecryptedValueFn, notificationService *notifications.NotificationService) (Notifier, error) {
	notifierPlugin, found := notifierFactories[model.Type]
	if !found {
		return nil, fmt.Errorf("unsupported notification type %q", model.Type)
	}

	return notifierPlugin.Factory(cfg, model, fn, notificationService)
}

// GetDecryptedValueFn is a function that returns the decrypted value of
// the given key. If the key is not present, then it returns the fallback value.
type GetDecryptedValueFn func(ctx context.Context, sjd map[string][]byte, key string, fallback string, secret string) string

// NotifierFactory is a signature for creating notifiers.
type NotifierFactory func(*setting.Cfg, *alertmodels.AlertNotification, GetDecryptedValueFn, notifications.Service) (Notifier, error)

var notifierFactories = make(map[string]*NotifierPlugin)

// RegisterNotifier registers a notifier.
func RegisterNotifier(plugin *NotifierPlugin) {
	notifierFactories[plugin.Type] = plugin
}

// GetNotifiers returns a list of metadata about available notifiers.
func GetNotifiers() []*NotifierPlugin {
	list := make([]*NotifierPlugin, 0)

	for _, value := range notifierFactories {
		list = append(list, value)
	}

	return list
}
