import React, { useState } from 'react';
import { Enable, Disable } from 'react-enable';

import { Tab, TabContent, TabsBar } from '@grafana/ui';

import { AlertingPageWrapper } from '../components/AlertingPageWrapper';
import { AlertingFeature } from '../features';

import GettingStarted, { WelcomeHeader } from './GettingStarted';
import Insights from './Insights';

type HomeTabs = 'insights' | 'gettingStarted';

export default function Home() {
  const [activeTab, setActiveTab] = useState<HomeTabs>('insights');

  return (
    <AlertingPageWrapper pageId={'alerting'}>
      <Enable feature={AlertingFeature.InsightsPage}>
        <WelcomeHeader />
        <TabsBar>
          <Tab
            key={'insights'}
            label={'Insights'}
            active={activeTab === 'insights'}
            onChangeTab={() => {
              setActiveTab('insights');
            }}
          />
          <Tab
            key={'gettingStarted'}
            label={'Overview'}
            active={activeTab === 'gettingStarted'}
            onChangeTab={() => {
              setActiveTab('gettingStarted');
            }}
          />
        </TabsBar>
        <TabContent>
          {activeTab === 'insights' && <Insights />}
          {activeTab === 'gettingStarted' && <GettingStarted />}
        </TabContent>
      </Enable>
      <Disable feature={AlertingFeature.InsightsPage}>
        <GettingStarted showWelcomeHeader={true} />
      </Disable>
    </AlertingPageWrapper>
  );
}
