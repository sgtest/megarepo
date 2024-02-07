package cloudwatch

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"reflect"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go/aws"
	"github.com/aws/aws-sdk-go/service/cloudwatchlogs"
	"github.com/aws/aws-sdk-go/service/ec2"
	"github.com/aws/aws-sdk-go/service/resourcegroupstaggingapi"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana/pkg/tsdb/cloudwatch/constants"
)

type suggestData struct {
	Text  string `json:"text"`
	Value string `json:"value"`
	Label string `json:"label,omitempty"`
}

func parseMultiSelectValue(input string) []string {
	trimmedInput := strings.TrimSpace(input)
	if strings.HasPrefix(trimmedInput, "{") {
		values := strings.Split(strings.TrimRight(strings.TrimLeft(trimmedInput, "{"), "}"), ",")
		trimmedValues := make([]string, len(values))
		for i, v := range values {
			trimmedValues[i] = strings.TrimSpace(v)
		}
		return trimmedValues
	}

	return []string{trimmedInput}
}

// Whenever this list is updated, the frontend list should also be updated.
// Please update the region list in public/app/plugins/datasource/cloudwatch/partials/config.html
func (e *cloudWatchExecutor) handleGetRegions(ctx context.Context, pluginCtx backend.PluginContext, parameters url.Values) ([]suggestData, error) {
	instance, err := e.getInstance(ctx, pluginCtx)
	if err != nil {
		return nil, err
	}

	profile := instance.Settings.Profile
	if cache, ok := e.regionCache.Load(profile); ok {
		if cache2, ok2 := cache.([]suggestData); ok2 {
			return cache2, nil
		}
	}

	client, err := e.getEC2Client(ctx, pluginCtx, defaultRegion)
	if err != nil {
		return nil, err
	}
	regions := constants.Regions()
	ec2Regions, err := client.DescribeRegionsWithContext(ctx, &ec2.DescribeRegionsInput{})
	if err != nil {
		// ignore error for backward compatibility
		e.logger.FromContext(ctx).Error("Failed to get regions", "error", err)
	} else {
		mergeEC2RegionsAndConstantRegions(regions, ec2Regions.Regions)
	}

	result := make([]suggestData, 0)
	for region := range regions {
		result = append(result, suggestData{Text: region, Value: region, Label: region})
	}
	sort.Slice(result, func(i, j int) bool {
		return result[i].Text < result[j].Text
	})
	e.regionCache.Store(profile, result)

	return result, nil
}

func mergeEC2RegionsAndConstantRegions(regions map[string]struct{}, ec2Regions []*ec2.Region) {
	for _, region := range ec2Regions {
		if _, ok := regions[*region.RegionName]; !ok {
			regions[*region.RegionName] = struct{}{}
		}
	}
}

func (e *cloudWatchExecutor) handleGetEbsVolumeIds(ctx context.Context, pluginCtx backend.PluginContext, parameters url.Values) ([]suggestData, error) {
	region := parameters.Get("region")
	instanceId := parameters.Get("instanceId")

	instanceIds := aws.StringSlice(parseMultiSelectValue(instanceId))
	instances, err := e.ec2DescribeInstances(ctx, pluginCtx, region, nil, instanceIds)
	if err != nil {
		return nil, err
	}

	result := make([]suggestData, 0)
	for _, reservation := range instances.Reservations {
		for _, instance := range reservation.Instances {
			for _, mapping := range instance.BlockDeviceMappings {
				result = append(result, suggestData{Text: *mapping.Ebs.VolumeId, Value: *mapping.Ebs.VolumeId, Label: *mapping.Ebs.VolumeId})
			}
		}
	}

	return result, nil
}

func (e *cloudWatchExecutor) handleGetEc2InstanceAttribute(ctx context.Context, pluginCtx backend.PluginContext, parameters url.Values) ([]suggestData, error) {
	region := parameters.Get("region")
	attributeName := parameters.Get("attributeName")
	filterJson := parameters.Get("filters")

	filterMap := map[string]any{}
	err := json.Unmarshal([]byte(filterJson), &filterMap)
	if err != nil {
		return nil, fmt.Errorf("error unmarshaling filter: %v", err)
	}

	var filters []*ec2.Filter
	for k, v := range filterMap {
		if vv, ok := v.([]any); ok {
			var values []*string
			for _, vvv := range vv {
				if vvvv, ok := vvv.(string); ok {
					values = append(values, &vvvv)
				}
			}
			filters = append(filters, &ec2.Filter{
				Name:   aws.String(k),
				Values: values,
			})
		}
	}

	instances, err := e.ec2DescribeInstances(ctx, pluginCtx, region, filters, nil)
	if err != nil {
		return nil, err
	}

	result := make([]suggestData, 0)
	dupCheck := make(map[string]bool)
	for _, reservation := range instances.Reservations {
		for _, instance := range reservation.Instances {
			tags := make(map[string]string)
			for _, tag := range instance.Tags {
				tags[*tag.Key] = *tag.Value
			}

			var data string
			if strings.Index(attributeName, "Tags.") == 0 {
				tagName := attributeName[5:]
				data = tags[tagName]
			} else {
				attributePath := strings.Split(attributeName, ".")
				v := reflect.ValueOf(instance)
				for _, key := range attributePath {
					if v.Kind() == reflect.Ptr {
						v = v.Elem()
					}
					if v.Kind() != reflect.Struct {
						return nil, errors.New("invalid attribute path")
					}
					v = v.FieldByName(key)
					if !v.IsValid() {
						return nil, errors.New("invalid attribute path")
					}
				}
				if attr, ok := v.Interface().(*string); ok {
					data = *attr
				} else if attr, ok := v.Interface().(*time.Time); ok {
					data = attr.String()
				} else {
					return nil, errors.New("invalid attribute path")
				}
			}

			if _, exists := dupCheck[data]; exists {
				continue
			}

			dupCheck[data] = true
			result = append(result, suggestData{Text: data, Value: data, Label: data})
		}
	}

	sort.Slice(result, func(i, j int) bool {
		return result[i].Text < result[j].Text
	})

	return result, nil
}

func (e *cloudWatchExecutor) handleGetResourceArns(ctx context.Context, pluginCtx backend.PluginContext, parameters url.Values) ([]suggestData, error) {
	region := parameters.Get("region")
	resourceType := parameters.Get("resourceType")
	tagsJson := parameters.Get("tags")

	tagsMap := map[string]any{}
	err := json.Unmarshal([]byte(tagsJson), &tagsMap)
	if err != nil {
		return nil, fmt.Errorf("error unmarshaling filter: %v", err)
	}

	var filters []*resourcegroupstaggingapi.TagFilter
	for k, v := range tagsMap {
		if vv, ok := v.([]any); ok {
			var values []*string
			for _, vvv := range vv {
				if vvvv, ok := vvv.(string); ok {
					values = append(values, &vvvv)
				}
			}
			filters = append(filters, &resourcegroupstaggingapi.TagFilter{
				Key:    aws.String(k),
				Values: values,
			})
		}
	}

	var resourceTypes []*string
	resourceTypes = append(resourceTypes, &resourceType)

	resources, err := e.resourceGroupsGetResources(ctx, pluginCtx, region, filters, resourceTypes)
	if err != nil {
		return nil, err
	}

	result := make([]suggestData, 0)
	for _, resource := range resources.ResourceTagMappingList {
		data := *resource.ResourceARN
		result = append(result, suggestData{Text: data, Value: data, Label: data})
	}

	return result, nil
}

func (e *cloudWatchExecutor) ec2DescribeInstances(ctx context.Context, pluginCtx backend.PluginContext, region string, filters []*ec2.Filter, instanceIds []*string) (*ec2.DescribeInstancesOutput, error) {
	params := &ec2.DescribeInstancesInput{
		Filters:     filters,
		InstanceIds: instanceIds,
	}

	client, err := e.getEC2Client(ctx, pluginCtx, region)
	if err != nil {
		return nil, err
	}

	var resp ec2.DescribeInstancesOutput
	if err := client.DescribeInstancesPagesWithContext(ctx, params, func(page *ec2.DescribeInstancesOutput, lastPage bool) bool {
		resp.Reservations = append(resp.Reservations, page.Reservations...)
		return !lastPage
	}); err != nil {
		return nil, fmt.Errorf("failed to call ec2:DescribeInstances, %w", err)
	}

	return &resp, nil
}

func (e *cloudWatchExecutor) resourceGroupsGetResources(ctx context.Context, pluginCtx backend.PluginContext, region string, filters []*resourcegroupstaggingapi.TagFilter,
	resourceTypes []*string) (*resourcegroupstaggingapi.GetResourcesOutput, error) {
	params := &resourcegroupstaggingapi.GetResourcesInput{
		ResourceTypeFilters: resourceTypes,
		TagFilters:          filters,
	}

	client, err := e.getRGTAClient(ctx, pluginCtx, region)
	if err != nil {
		return nil, err
	}

	var resp resourcegroupstaggingapi.GetResourcesOutput
	if err := client.GetResourcesPagesWithContext(ctx, params,
		func(page *resourcegroupstaggingapi.GetResourcesOutput, lastPage bool) bool {
			resp.ResourceTagMappingList = append(resp.ResourceTagMappingList, page.ResourceTagMappingList...)
			return !lastPage
		}); err != nil {
		return nil, fmt.Errorf("failed to call tag:GetResources, %w", err)
	}

	return &resp, nil
}

// legacy route, will be removed once GovCloud supports Cross Account Observability
func (e *cloudWatchExecutor) handleGetLogGroups(ctx context.Context, pluginCtx backend.PluginContext, parameters url.Values) ([]suggestData, error) {
	region := parameters.Get("region")
	limit := parameters.Get("limit")
	logGroupNamePrefix := parameters.Get("logGroupNamePrefix")

	logsClient, err := e.getCWLogsClient(ctx, pluginCtx, region)
	if err != nil {
		return nil, err
	}

	logGroupLimit := defaultLogGroupLimit
	intLimit, err := strconv.ParseInt(limit, 10, 64)
	if err == nil && intLimit > 0 {
		logGroupLimit = intLimit
	}

	input := &cloudwatchlogs.DescribeLogGroupsInput{Limit: aws.Int64(logGroupLimit)}
	if len(logGroupNamePrefix) > 0 {
		input.LogGroupNamePrefix = aws.String(logGroupNamePrefix)
	}
	var response *cloudwatchlogs.DescribeLogGroupsOutput
	response, err = logsClient.DescribeLogGroupsWithContext(ctx, input)
	if err != nil || response == nil {
		return nil, err
	}
	result := make([]suggestData, 0)
	for _, logGroup := range response.LogGroups {
		logGroupName := *logGroup.LogGroupName
		result = append(result, suggestData{Text: logGroupName, Value: logGroupName, Label: logGroupName})
	}

	return result, nil
}
