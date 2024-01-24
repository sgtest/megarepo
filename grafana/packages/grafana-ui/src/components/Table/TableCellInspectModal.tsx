import { isString } from 'lodash';
import React from 'react';

import { ClipboardButton } from '../ClipboardButton/ClipboardButton';
import { Modal } from '../Modal/Modal';
import { CodeEditor } from '../Monaco/CodeEditor';

interface TableCellInspectModalProps {
  value: any;
  onDismiss: () => void;
  mode: 'code' | 'text';
}

export function TableCellInspectModal({ value, onDismiss, mode }: TableCellInspectModalProps) {
  let displayValue = value;
  if (isString(value)) {
    const trimmedValue = value.trim();
    // Exclude numeric strings like '123' from being displayed in code/JSON mode
    if (trimmedValue[0] === '{' || trimmedValue[0] === '[' || mode === 'code') {
      try {
        value = JSON.parse(value);
        mode = 'code';
      } catch {
        mode = 'text';
      } // ignore errors
    } else {
      mode = 'text';
    }
  } else {
    displayValue = JSON.stringify(value, null, ' ');
  }
  let text = displayValue;

  if (mode === 'code') {
    text = JSON.stringify(value, null, ' ');
  }

  return (
    <Modal onDismiss={onDismiss} isOpen={true} title="Inspect value">
      {mode === 'code' ? (
        <CodeEditor
          width="100%"
          height={500}
          language="json"
          showLineNumbers={true}
          showMiniMap={(text && text.length) > 100}
          value={text}
          readOnly={true}
        />
      ) : (
        <pre>{text}</pre>
      )}
      <Modal.ButtonRow>
        <ClipboardButton icon="copy" getText={() => text}>
          Copy to Clipboard
        </ClipboardButton>
      </Modal.ButtonRow>
    </Modal>
  );
}
