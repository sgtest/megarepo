import { test, expect } from '@playwright/test';
import translations from '../client/i18n/locales/english/translations.json';
test.use({ storageState: 'playwright/.auth/certified-user.json' });

const settingsObject = {
  usernamePlaceholder: '{{username}}',
  usernameUpdateToLowerCase: 'quincy',
  usernameUpdateToUpperCase: 'Mrugesh',
  usernameAvailable: 'Sem',
  usernameAvailablePressingEnter: 'Oliver',
  usernameNotAvailable: 'Twaha',
  usernameInvalid: 'user!',
  usernameTooShort: 'us',
  certifiedUsername: 'certifieduser',
  testUser: 'testuser',
  errorCode: '404'
};

test.describe('Username Settings Validation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
  });

  test('Should display Username Input and Save Button', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    const saveButton = page.getByRole('button', {
      name: translations.settings.labels.username
    });
    await expect(inputLabel).toBeVisible();
    await expect(saveButton).toBeVisible();
  });

  test('Should handle Reserved Username Error', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    await inputLabel.fill(settingsObject.errorCode);
    await expect(
      page.getByText(
        translations.settings.username['is a reserved error code'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.errorCode
        )
      )
    ).toBeVisible();
  });

  test('Should handle Invalid Username Error', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    await inputLabel.fill(settingsObject.usernameInvalid);
    await expect(
      page.getByText(
        translations.settings.username['contains invalid characters'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.usernameInvalid
        )
      )
    ).toBeVisible();
  });

  test('Should handle Unavailable Username Error', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    await inputLabel.fill(settingsObject.usernameNotAvailable);
    await expect(
      page.getByText(
        translations.settings.username['unavailable'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.usernameNotAvailable
        )
      )
    ).toBeVisible();
  });

  test('Should handle Too Short Username Error', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    await inputLabel.fill(settingsObject.usernameTooShort);
    await expect(
      page.getByText(
        translations.settings.username['is too short'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.usernameTooShort
        )
      )
    ).toBeVisible();
  });

  test('Should save valid Username', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    const saveButton = page.getByRole('button', {
      name: translations.settings.labels.username
    });
    await inputLabel.fill(settingsObject.usernameAvailable);
    await expect(saveButton).not.toBeDisabled();
    await saveButton.click();
    await expect(
      page.getByText(
        translations.flash['username-updated'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.usernameAvailable
        )
      )
    ).toBeVisible();
  });

  test('should update username in lowercase and reflect in the UI', async ({
    page
  }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    const saveButton = page.getByRole('button', {
      name: translations.settings.labels.username
    });
    await inputLabel.fill(settingsObject.usernameUpdateToLowerCase);
    await expect(saveButton).not.toBeDisabled();
    await saveButton.click();
    await expect(
      page.getByText(
        translations.flash['username-updated'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.usernameUpdateToLowerCase
        )
      )
    ).toBeVisible();
  });

  test('should update username in uppercase and reflect in the UI', async ({
    page
  }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    const saveButton = page.getByRole('button', {
      name: translations.settings.labels.username
    });
    await inputLabel.fill(settingsObject.usernameUpdateToUpperCase);
    await expect(saveButton).not.toBeDisabled();
    await saveButton.click();
    await expect(
      page.getByText(
        translations.flash['username-updated'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.usernameUpdateToUpperCase
        )
      )
    ).toBeVisible();
  });

  test('should update username by pressing enter', async ({ page }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    await inputLabel.fill(settingsObject.testUser);

    await expect(
      page.getByText(translations.settings.username.available)
    ).toBeVisible();

    await inputLabel.press('Enter');

    await expect(
      page.getByText(
        translations.flash['username-updated'].replace(
          settingsObject.usernamePlaceholder,
          settingsObject.testUser
        )
      )
    ).toBeVisible();
  });

  test('should not be able to update username to the same username', async ({
    page
  }) => {
    const inputLabel = page.getByLabel(translations.settings.labels.username);
    const saveButton = page.getByRole('button', {
      name: translations.settings.labels.username
    });
    await inputLabel.fill(settingsObject.testUser);
    await expect(saveButton).toBeDisabled();
  });
});
