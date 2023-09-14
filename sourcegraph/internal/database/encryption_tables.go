package database

import (
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
)

type EncryptionConfig struct {
	TableName           string
	IDFieldName         string
	KeyIDFieldName      string
	EncryptedFieldNames []string
	UpdateAsBytes       bool
	Scan                func(basestore.Rows, error) (map[int]Encrypted, error)
	TreatEmptyAsNull    bool
	Key                 func() encryption.Key
	Limit               int
}

var EncryptionConfigs = []EncryptionConfig{
	externalServicesEncryptionConfig,
	userExternalAccountsEncryptionConfig,
	userCredentialsEncryptionConfig,
	batchChangesSiteCredentialsEncryptionConfig,
	webhooklogsEncryptionConfig,
	executorSecretsEncryptionConfig,
	outboundWebhooksEncryptionConfig,
}

var externalServicesEncryptionConfig = EncryptionConfig{
	TableName:           "external_services",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"config"},
	Scan:                basestore.NewMapScanner(scanEncryptedString),
	Key:                 func() encryption.Key { return keyring.Default().ExternalServiceKey },
	Limit:               100,
}

var userExternalAccountsEncryptionConfig = EncryptionConfig{
	TableName:           "user_external_accounts",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"auth_data", "account_data"},
	Scan:                basestore.NewMapScanner(scanNullableEncryptedStringPair),
	TreatEmptyAsNull:    true,
	Key:                 func() encryption.Key { return keyring.Default().UserExternalAccountKey },
	Limit:               100,
}

var userCredentialsEncryptionConfig = EncryptionConfig{
	TableName:           "user_credentials",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"credential"},
	UpdateAsBytes:       true,
	Scan:                basestore.NewMapScanner(scanEncryptedBytea),
	Key:                 func() encryption.Key { return keyring.Default().BatchChangesCredentialKey },
	Limit:               5,
}

var batchChangesSiteCredentialsEncryptionConfig = EncryptionConfig{
	TableName:           "batch_changes_site_credentials",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"credential"},
	UpdateAsBytes:       true,
	Scan:                basestore.NewMapScanner(scanEncryptedBytea),
	Key:                 func() encryption.Key { return keyring.Default().BatchChangesCredentialKey },
	Limit:               5,
}

var webhooklogsEncryptionConfig = EncryptionConfig{
	TableName:           "webhook_logs",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"request", "response"},
	Scan:                basestore.NewMapScanner(scanEncryptedStringPair),
	Key:                 func() encryption.Key { return keyring.Default().WebhookLogKey },
	Limit:               5,
}

var executorSecretsEncryptionConfig = EncryptionConfig{
	TableName:           "executor_secrets",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"value"},
	UpdateAsBytes:       true,
	Scan:                basestore.NewMapScanner(scanEncryptedBytea),
	Key:                 func() encryption.Key { return keyring.Default().ExecutorSecretKey },
	Limit:               5,
}

var outboundWebhooksEncryptionConfig = EncryptionConfig{
	TableName:           "outbound_webhooks",
	IDFieldName:         "id",
	KeyIDFieldName:      "encryption_key_id",
	EncryptedFieldNames: []string{"url", "secret"},
	Scan:                basestore.NewMapScanner(scanEncryptedStringPair),
	Key:                 func() encryption.Key { return keyring.Default().OutboundWebhookKey },
	Limit:               5,
}

func scanEncryptedString(scanner dbutil.Scanner) (id int, e Encrypted, err error) {
	e.Values = make([]string, 1)
	err = scanner.Scan(&id, &e.KeyID, &e.Values[0])
	return
}

func scanNullableEncryptedString(scanner dbutil.Scanner) (id int, e Encrypted, err error) {
	e.Values = make([]string, 1)
	err = scanner.Scan(&id, &e.KeyID, &dbutil.NullString{S: &e.Values[0]})
	return
}

func scanEncryptedStringPair(scanner dbutil.Scanner) (id int, e Encrypted, err error) {
	e.Values = make([]string, 2)
	err = scanner.Scan(&id, &e.KeyID, &e.Values[0], &e.Values[1])
	return
}

func scanNullableEncryptedStringPair(scanner dbutil.Scanner) (id int, e Encrypted, err error) {
	e.Values = make([]string, 2)
	err = scanner.Scan(&id, &e.KeyID, &dbutil.NullString{S: &e.Values[0]}, &dbutil.NullString{S: &e.Values[1]})
	return
}

func scanEncryptedBytea(scanner dbutil.Scanner) (id int, e Encrypted, err error) {
	var bs []byte
	err = scanner.Scan(&id, &e.KeyID, &bs)
	e.Values = []string{string(bs)}
	return
}
