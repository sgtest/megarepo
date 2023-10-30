/*
Copyright 2021 The Kubernetes Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package v1beta3

import (
	"sort"

	"k8s.io/apimachinery/pkg/conversion"

	"k8s.io/kubernetes/cmd/kubeadm/app/apis/kubeadm"
)

// Convert_kubeadm_InitConfiguration_To_v1beta3_InitConfiguration converts a private InitConfiguration to public InitConfiguration.
func Convert_kubeadm_InitConfiguration_To_v1beta3_InitConfiguration(in *kubeadm.InitConfiguration, out *InitConfiguration, s conversion.Scope) error {
	return autoConvert_kubeadm_InitConfiguration_To_v1beta3_InitConfiguration(in, out, s)
}

// Convert_kubeadm_JoinConfiguration_To_v1beta3_JoinConfiguration converts a private JoinConfiguration to public JoinConfiguration.
func Convert_kubeadm_JoinConfiguration_To_v1beta3_JoinConfiguration(in *kubeadm.JoinConfiguration, out *JoinConfiguration, s conversion.Scope) error {
	return autoConvert_kubeadm_JoinConfiguration_To_v1beta3_JoinConfiguration(in, out, s)
}

// Convert_v1beta3_InitConfiguration_To_kubeadm_InitConfiguration converts a public InitConfiguration to private InitConfiguration.
func Convert_v1beta3_InitConfiguration_To_kubeadm_InitConfiguration(in *InitConfiguration, out *kubeadm.InitConfiguration, s conversion.Scope) error {
	err := autoConvert_v1beta3_InitConfiguration_To_kubeadm_InitConfiguration(in, out, s)
	if err != nil {
		return err
	}
	err = Convert_v1beta3_ClusterConfiguration_To_kubeadm_ClusterConfiguration(&ClusterConfiguration{}, &out.ClusterConfiguration, s)
	// Required to pass fuzzer tests. This ClusterConfiguration is empty and is never defaulted.
	// If we call Convert_v1beta3_ClusterConfiguration_To_kubeadm_ClusterConfiguration() it will receive
	// a default value, thus here we need to reset it back to "".
	out.EncryptionAlgorithm = ""
	return err
}

// Convert_kubeadm_ClusterConfiguration_To_v1beta3_ClusterConfiguration is required due to missing EncryptionAlgorithm in v1beta3.
func Convert_kubeadm_ClusterConfiguration_To_v1beta3_ClusterConfiguration(in *kubeadm.ClusterConfiguration, out *ClusterConfiguration, s conversion.Scope) error {
	return autoConvert_kubeadm_ClusterConfiguration_To_v1beta3_ClusterConfiguration(in, out, s)
}

// Convert_v1beta3_ClusterConfiguration_To_kubeadm_ClusterConfiguration is required due to missing EncryptionAlgorithm in v1beta3.
func Convert_v1beta3_ClusterConfiguration_To_kubeadm_ClusterConfiguration(in *ClusterConfiguration, out *kubeadm.ClusterConfiguration, s conversion.Scope) error {
	// Required to pass validation and fuzzer tests. The field is missing in v1beta3, thus we have to
	// default it to a sane (default) value in the internal type.
	out.EncryptionAlgorithm = kubeadm.EncryptionAlgorithmRSA
	return autoConvert_v1beta3_ClusterConfiguration_To_kubeadm_ClusterConfiguration(in, out, s)
}

// Convert_v1beta3_ControlPlaneComponent_To_kubeadm_ControlPlaneComponent is required due to the missing ControlPlaneComponent.ExtraEnvs in v1beta3.
func Convert_v1beta3_ControlPlaneComponent_To_kubeadm_ControlPlaneComponent(in *ControlPlaneComponent, out *kubeadm.ControlPlaneComponent, s conversion.Scope) error {
	out.ExtraEnvs = []kubeadm.EnvVar{}
	out.ExtraArgs = convertToArgs(in.ExtraArgs)
	return autoConvert_v1beta3_ControlPlaneComponent_To_kubeadm_ControlPlaneComponent(in, out, s)
}

// Convert_kubeadm_ControlPlaneComponent_To_v1beta3_ControlPlaneComponent converts a private ControlPlaneComponent to public ControlPlaneComponent.
func Convert_kubeadm_ControlPlaneComponent_To_v1beta3_ControlPlaneComponent(in *kubeadm.ControlPlaneComponent, out *ControlPlaneComponent, s conversion.Scope) error {
	out.ExtraArgs = convertFromArgs(in.ExtraArgs)
	return autoConvert_kubeadm_ControlPlaneComponent_To_v1beta3_ControlPlaneComponent(in, out, s)
}

// Convert_v1beta3_LocalEtcd_To_kubeadm_LocalEtcd is required due to the missing LocalEtcd.ExtraEnvs in v1beta3.
func Convert_v1beta3_LocalEtcd_To_kubeadm_LocalEtcd(in *LocalEtcd, out *kubeadm.LocalEtcd, s conversion.Scope) error {
	out.ExtraEnvs = []kubeadm.EnvVar{}
	out.ExtraArgs = convertToArgs(in.ExtraArgs)
	return autoConvert_v1beta3_LocalEtcd_To_kubeadm_LocalEtcd(in, out, s)
}

// Convert_kubeadm_LocalEtcd_To_v1beta3_LocalEtcd converts a private LocalEtcd to public LocalEtcd.
func Convert_kubeadm_LocalEtcd_To_v1beta3_LocalEtcd(in *kubeadm.LocalEtcd, out *LocalEtcd, s conversion.Scope) error {
	out.ExtraArgs = convertFromArgs(in.ExtraArgs)
	return autoConvert_kubeadm_LocalEtcd_To_v1beta3_LocalEtcd(in, out, s)
}

// Convert_v1beta3_NodeRegistrationOptions_To_kubeadm_NodeRegistrationOptions converts a public NodeRegistrationOptions to private NodeRegistrationOptions.
func Convert_v1beta3_NodeRegistrationOptions_To_kubeadm_NodeRegistrationOptions(in *NodeRegistrationOptions, out *kubeadm.NodeRegistrationOptions, s conversion.Scope) error {
	out.KubeletExtraArgs = convertToArgs(in.KubeletExtraArgs)
	return autoConvert_v1beta3_NodeRegistrationOptions_To_kubeadm_NodeRegistrationOptions(in, out, s)
}

// Convert_kubeadm_NodeRegistrationOptions_To_v1beta3_NodeRegistrationOptions converts a private NodeRegistrationOptions to public NodeRegistrationOptions.
func Convert_kubeadm_NodeRegistrationOptions_To_v1beta3_NodeRegistrationOptions(in *kubeadm.NodeRegistrationOptions, out *NodeRegistrationOptions, s conversion.Scope) error {
	out.KubeletExtraArgs = convertFromArgs(in.KubeletExtraArgs)
	return autoConvert_kubeadm_NodeRegistrationOptions_To_v1beta3_NodeRegistrationOptions(in, out, s)
}

// Convert_kubeadm_DNS_To_v1beta3_DNS converts a private DNS to public DNS.
func Convert_kubeadm_DNS_To_v1beta3_DNS(in *kubeadm.DNS, out *DNS, s conversion.Scope) error {
	return autoConvert_kubeadm_DNS_To_v1beta3_DNS(in, out, s)
}

// convertToArgs takes a argument map and converts it to a slice of arguments.
// Te resulting argument slice is sorted alpha-numerically.
func convertToArgs(in map[string]string) []kubeadm.Arg {
	if in == nil {
		return nil
	}
	args := make([]kubeadm.Arg, 0, len(in))
	for k, v := range in {
		args = append(args, kubeadm.Arg{Name: k, Value: v})
	}
	sort.Slice(args, func(i, j int) bool {
		if args[i].Name == args[j].Name {
			return args[i].Value < args[j].Value
		}
		return args[i].Name < args[j].Name
	})
	return args
}

// convertFromArgs takes a slice of arguments and returns an argument map.
// Duplicate argument keys will be de-duped, where later keys will take precedence.
func convertFromArgs(in []kubeadm.Arg) map[string]string {
	if in == nil {
		return nil
	}
	args := make(map[string]string, len(in))
	for _, arg := range in {
		args[arg.Name] = arg.Value
	}
	return args
}
