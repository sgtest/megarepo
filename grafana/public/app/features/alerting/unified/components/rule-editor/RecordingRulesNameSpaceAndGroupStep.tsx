import React from 'react';
import { useFormContext } from 'react-hook-form';

import { RuleFormValues } from '../../types/rule-form';

import { GroupAndNamespaceFields } from './GroupAndNamespaceFields';
import { RuleEditorSection } from './RuleEditorSection';

export function RecordingRulesNameSpaceAndGroupStep() {
  const { watch } = useFormContext<RuleFormValues & { location?: string }>();

  const dataSourceName = watch('dataSourceName');

  if (!dataSourceName) {
    return null;
  }

  return (
    <RuleEditorSection
      stepNo={3}
      title={'Add namespace and group'}
      description="Select the Namespace and Group for your recording rule."
    >
      <GroupAndNamespaceFields rulesSourceName={dataSourceName} />
    </RuleEditorSection>
  );
}
