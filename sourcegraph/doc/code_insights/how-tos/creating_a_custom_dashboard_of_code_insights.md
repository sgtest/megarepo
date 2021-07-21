# Creating a custom dashboard of code insights

This how-to assumes that you already have created some code insights to add to a dashboard. If you have yet to create any code insights, start with the [quickstart](../quickstart.md) guide. 

## 1. Navigate to the Code Insights page 

Start on the code insights page by clicking the Code Insights navbar item or going to `/insights/dashboards/all`. 

## 2. Create and name a new dashboard

Click "Create new dashboard" in the top right corner and name your dashboard. Dashboard names must be unique. 

## 3. Select a visibility level 

Set a visibility level for your dashboard. Dashboards [respect insights' permissions](../explanations/viewing_code_insights.md#dashboard-visibility-respects-insights-visibility), so don't create an organization-shared dashboard if you have private insights you want to attach to it. 

- Private: visible only to you 
- Shared with an organization: visible to everyone in the organization 
- Global: visible to everyone on the Sourcegraph instance 

> Global visibility is currently only available if your instance is not [using a separate global settings file](../../admin/config/advanced_config_file.md#global-settings). Global visibility regardless of settings file setup will arrive by October 2021. 

> The **quick workaround** is to [make an organization and easily add all users to it](../../admin/organizations.md). 

Then click "Create dashboard." 

## 4. Add insights to your new, empty dashboard 

Click the "Add insight" box on the empty dashboard view to pull up the add insights modal. You can also always pull this modal up with the contextual three-dots click to the right of the dashboard dropdown picker, via the "Add or remove insights" option. 

Select which insights you want to add. Insights won't be added until you click save. Un-checking an insight will remove that insight after you click save. 

## 5. Share your dashboard

You can share your dashboard via the url, or by copying this same url in the "copy link" menu item next to the dashboards picker. 


