import { SelectableValue } from '@grafana/data';

import { fieldMap } from '../fields';
import { FieldData, SSOProvider, SSOProviderDTO } from '../types';

import { isSelectableValue } from './guards';

export const emptySettings: SSOProviderDTO = {
  allowAssignGrafanaAdmin: false,
  allowSignUp: false,
  allowedDomains: [],
  allowedGroups: [],
  allowedOrganizations: [],
  apiUrl: '',
  authStyle: '',
  authUrl: '',
  autoLogin: false,
  clientId: '',
  clientSecret: '',
  emailAttributeName: '',
  emailAttributePath: '',
  emptyScopes: false,
  enabled: false,
  extra: {},
  groupsAttributePath: '',
  hostedDomain: '',
  icon: 'shield',
  name: '',
  roleAttributePath: '',
  roleAttributeStrict: false,
  scopes: [],
  signoutRedirectUrl: '',
  skipOrgRoleSync: false,
  teamIds: [],
  teamIdsAttributePath: '',
  teamsUrl: '',
  tlsClientCa: '',
  tlsClientCert: '',
  tlsClientKey: '',
  tlsSkipVerify: false,
  tokenUrl: '',
  type: '',
  usePKCE: false,
  useRefreshToken: false,
};

const strToValue = (val: string | string[]): SelectableValue[] => {
  if (!val?.length) {
    return [];
  }
  if (Array.isArray(val)) {
    return val.map((v) => ({ label: v, value: v }));
  }
  return val.split(/[\s,]/).map((s) => ({ label: s, value: s }));
};

export function dataToDTO(data?: SSOProvider): SSOProviderDTO {
  if (!data) {
    return emptySettings;
  }
  const arrayFields = getArrayFields(fieldMap);
  const settings = { ...data.settings };
  for (const field of arrayFields) {
    //@ts-expect-error
    settings[field] = strToValue(settings[field]);
  }
  //@ts-expect-error
  return settings;
}

const valuesToString = (values: Array<SelectableValue<string>>) => {
  return values.map(({ value }) => value).join(',');
};

// Convert the DTO to the data format used by the API
export function dtoToData(dto: SSOProviderDTO) {
  const arrayFields = getArrayFields(fieldMap);
  const settings = { ...dto };

  for (const field of arrayFields) {
    const value = dto[field];
    if (value && isSelectableValue(value)) {
      //@ts-expect-error
      settings[field] = valuesToString(value);
    }
  }
  return settings;
}

export function getArrayFields(obj: Record<string, FieldData>): Array<keyof SSOProviderDTO> {
  return Object.entries(obj)
    .filter(([_, value]) => value.type === 'select' && value.multi === true)
    .map(([key]) => key as keyof SSOProviderDTO);
}
