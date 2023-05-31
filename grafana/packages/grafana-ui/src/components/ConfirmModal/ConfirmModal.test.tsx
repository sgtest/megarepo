import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import React from 'react';

import { ConfirmModal } from './ConfirmModal';

describe('ConfirmModal', () => {
  it('should render correct title, body, dismiss-, alternative- and confirm-text', () => {
    render(
      <ConfirmModal
        title="Some Title"
        body="Some Body"
        confirmText="Please Confirm"
        alternativeText="Alternative Text"
        dismissText="Dismiss Text"
        isOpen={true}
        onConfirm={() => {}}
        onDismiss={() => {}}
        onAlternative={() => {}}
      />
    );

    expect(screen.getByRole('heading', { name: 'Some Title' })).toBeInTheDocument();
    expect(screen.getByText('Some Body')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Dismiss Text' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Alternative Text' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Please Confirm' })).toBeInTheDocument();
  });

  it('should render nothing when isOpen is false', () => {
    render(
      <ConfirmModal
        title="Some Title"
        body="Some Body"
        confirmText="Confirm"
        isOpen={false}
        onConfirm={() => {}}
        onDismiss={() => {}}
      />
    );

    expect(screen.queryByRole('heading', { name: 'Some Title' })).not.toBeInTheDocument();
    expect(screen.queryByText('Some Body')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Dismiss Text' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Alternative Text' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Confirm' })).not.toBeInTheDocument();
  });

  it('disables the confirm button initially when confirmation text is present', () => {
    render(
      <ConfirmModal
        title="Some Title"
        body="Some Body"
        confirmText="Please Confirm"
        alternativeText="Alternative Text"
        dismissText="Dismiss Text"
        isOpen={true}
        confirmationText="My confirmation text"
        onConfirm={() => {}}
        onDismiss={() => {}}
        onAlternative={() => {}}
      />
    );

    expect(screen.getByRole('button', { name: 'Please Confirm' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Please Confirm' })).toBeDisabled();
  });

  it('typing the confirmation text should enable the confirm button regardless of case', async () => {
    render(
      <ConfirmModal
        title="Some Title"
        body="Some Body"
        confirmText="Please Confirm"
        alternativeText="Alternative Text"
        dismissText="Dismiss Text"
        isOpen={true}
        confirmationText="My confirmation text"
        onConfirm={() => {}}
        onDismiss={() => {}}
        onAlternative={() => {}}
      />
    );

    expect(screen.getByRole('button', { name: 'Please Confirm' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Please Confirm' })).toBeDisabled();

    await userEvent.type(screen.getByPlaceholderText('Type "My confirmation text" to confirm'), 'mY CoNfIrMaTiOn TeXt');
    expect(screen.getByRole('button', { name: 'Please Confirm' })).not.toBeDisabled();
  });
});
