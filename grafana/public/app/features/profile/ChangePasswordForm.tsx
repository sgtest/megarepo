import { css } from '@emotion/css';
import React from 'react';

import { Button, Field, Form, HorizontalGroup, LinkButton } from '@grafana/ui';
import config from 'app/core/config';
import { t, Trans } from 'app/core/internationalization';
import { UserDTO } from 'app/types';

import { PasswordField } from '../../core/components/PasswordField/PasswordField';

import { ChangePasswordFields } from './types';

export interface Props {
  user: UserDTO;
  isSaving: boolean;
  onChangePassword: (payload: ChangePasswordFields) => void;
}

export const ChangePasswordForm = ({ user, onChangePassword, isSaving }: Props) => {
  const { disableLoginForm } = config;
  const authSource = user.authLabels?.length && user.authLabels[0];

  if (authSource === 'LDAP' || authSource === 'Auth Proxy') {
    return (
      <p>
        <Trans i18nKey="profile.change-password.ldap-auth-proxy-message">
          You cannot change password when signed in with LDAP or auth proxy.
        </Trans>
      </p>
    );
  }
  if (authSource && disableLoginForm) {
    return (
      <p>
        <Trans i18nKey="profile.change-password.cannot-change-password-message">Password cannot be changed here.</Trans>
      </p>
    );
  }

  return (
    <div
      className={css`
        max-width: 400px;
      `}
    >
      <Form onSubmit={onChangePassword}>
        {({ register, errors, getValues }) => {
          return (
            <>
              <Field
                label={t('profile.change-password.old-password-label', 'Old password')}
                invalid={!!errors.oldPassword}
                error={errors?.oldPassword?.message}
              >
                <PasswordField
                  id="current-password"
                  autoComplete="current-password"
                  {...register('oldPassword', {
                    required: t('profile.change-password.old-password-required', 'Old password is required'),
                  })}
                />
              </Field>

              <Field
                label={t('profile.change-password.new-password-label', 'New password')}
                invalid={!!errors.newPassword}
                error={errors?.newPassword?.message}
              >
                <PasswordField
                  id="new-password"
                  autoComplete="new-password"
                  {...register('newPassword', {
                    required: t('profile.change-password.new-password-required', 'New password is required'),
                    validate: {
                      confirm: (v) =>
                        v === getValues().confirmNew ||
                        t('profile.change-password.passwords-must-match', 'Passwords must match'),
                      old: (v) =>
                        v !== getValues().oldPassword ||
                        t(
                          'profile.change-password.new-password-same-as-old',
                          "New password can't be the same as the old one."
                        ),
                    },
                  })}
                />
              </Field>

              <Field
                label={t('profile.change-password.confirm-password-label', 'Confirm password')}
                invalid={!!errors.confirmNew}
                error={errors?.confirmNew?.message}
              >
                <PasswordField
                  id="confirm-new-password"
                  autoComplete="new-password"
                  {...register('confirmNew', {
                    required: t(
                      'profile.change-password.confirm-password-required',
                      'New password confirmation is required'
                    ),
                    validate: (v) =>
                      v === getValues().newPassword ||
                      t('profile.change-password.passwords-must-match', 'Passwords must match'),
                  })}
                />
              </Field>
              <HorizontalGroup>
                <Button variant="primary" disabled={isSaving} type="submit">
                  <Trans i18nKey="profile.change-password.change-password-button">Change Password</Trans>
                </Button>
                <LinkButton variant="secondary" href={`${config.appSubUrl}/profile`} fill="outline">
                  <Trans i18nKey="profile.change-password.cancel-button">Cancel</Trans>
                </LinkButton>
              </HorizontalGroup>
            </>
          );
        }}
      </Form>
    </div>
  );
};
