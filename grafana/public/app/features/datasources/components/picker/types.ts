import React from 'react';
import { DropzoneOptions } from 'react-dropzone';
import { Observable } from 'rxjs';

import { DataSourceInstanceSettings } from '@grafana/data';
import { DataSourceJsonData, DataSourceRef } from '@grafana/schema';

export interface DataSourceDropdownProps {
  onChange: (ds: DataSourceInstanceSettings<DataSourceJsonData>) => void;
  current: DataSourceInstanceSettings<DataSourceJsonData> | string | DataSourceRef | null | undefined;
  enableFileUpload?: boolean;
  fileUploadOptions?: DropzoneOptions;
  onClickAddCSV?: () => void;
  recentlyUsed?: string[];
  hideTextValue?: boolean;
  dashboard?: boolean;
  mixed?: boolean;
  width?: number;
}

export interface PickerContentProps extends DataSourceDropdownProps {
  keyboardEvents: Observable<React.KeyboardEvent>;
  style: React.CSSProperties;
  filterTerm?: string;
  dashboard?: boolean;
  mixed?: boolean;
  onClose: () => void;
  onDismiss: () => void;
}
