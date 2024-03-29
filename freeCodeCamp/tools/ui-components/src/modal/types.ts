import { type ReactNode } from 'react';

export interface ModalProps {
  children: ReactNode;
  open: boolean;
  onClose: () => void;
  size?: 'large' | 'medium';
  variant?: 'default' | 'danger';
}

export interface HeaderProps {
  children: ReactNode;
  showCloseButton?: boolean;
  /**
   * /learn uses the `button` selector to override Bootstrap button styles,
   * and the selector also affects `ui-components`.
   * We need to forward the class specific to the close button from /learn
   * in order for the button to display properly.
   * Ref: https://github.com/freeCodeCamp/freeCodeCamp/blob/c1137b0900ad098fee2fca430d11f69dfc909d3b/client/src/components/layouts/global.css#L179-L192
   * NOTE: This prop is temporary and should be removed once we remove all CSS overrides from /learn.
   * Tracking issue: https://github.com/freeCodeCamp/freeCodeCamp/issues/52715
   */
  closeButtonClassNames?: string;
}

export interface BodyProps {
  children: ReactNode;

  /**
   * Use `start` for better right-to-left support.
   * Use `left` if the modal body contains code blocks.
   */
  alignment?: 'center' | 'left' | 'start';
}

export interface FooterProps {
  children: ReactNode;
  alignment?: 'center' | 'end';
}
