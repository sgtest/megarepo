/*
Copyright The Kubernetes Authors.

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

// Code generated by protoc-gen-gogo. DO NOT EDIT.
// source: api.proto

/*
Package pluginregistration is a generated protocol buffer package.

It is generated from these files:

	api.proto

It has these top-level messages:

	PluginInfo
	RegistrationStatus
	RegistrationStatusResponse
	InfoRequest
*/
package pluginregistration

import proto "github.com/gogo/protobuf/proto"
import fmt "fmt"
import math "math"
import _ "github.com/gogo/protobuf/gogoproto"

import (
	context "golang.org/x/net/context"
	grpc "google.golang.org/grpc"
)

import strings "strings"
import reflect "reflect"

import io "io"

// Reference imports to suppress errors if they are not otherwise used.
var _ = proto.Marshal
var _ = fmt.Errorf
var _ = math.Inf

// This is a compile-time assertion to ensure that this generated file
// is compatible with the proto package it is being compiled against.
// A compilation error at this line likely means your copy of the
// proto package needs to be updated.
const _ = proto.GoGoProtoPackageIsVersion2 // please upgrade the proto package

// PluginInfo is the message sent from a plugin to the Kubelet pluginwatcher for plugin registration
type PluginInfo struct {
	// Type of the Plugin. CSIPlugin or DevicePlugin
	Type string `protobuf:"bytes,1,opt,name=type,proto3" json:"type,omitempty"`
	// Plugin name that uniquely identifies the plugin for the given plugin type.
	// For DevicePlugin, this is the resource name that the plugin manages and
	// should follow the extended resource name convention.
	// For CSI, this is the CSI driver registrar name.
	Name string `protobuf:"bytes,2,opt,name=name,proto3" json:"name,omitempty"`
	// Optional endpoint location. If found set by Kubelet component,
	// Kubelet component will use this endpoint for specific requests.
	// This allows the plugin to register using one endpoint and possibly use
	// a different socket for control operations. CSI uses this model to delegate
	// its registration external from the plugin.
	Endpoint string `protobuf:"bytes,3,opt,name=endpoint,proto3" json:"endpoint,omitempty"`
	// Plugin service API versions the plugin supports.
	// For DevicePlugin, this maps to the deviceplugin API versions the
	// plugin supports at the given socket.
	// The Kubelet component communicating with the plugin should be able
	// to choose any preferred version from this list, or returns an error
	// if none of the listed versions is supported.
	SupportedVersions []string `protobuf:"bytes,4,rep,name=supported_versions,json=supportedVersions" json:"supported_versions,omitempty"`
}

func (m *PluginInfo) Reset()                    { *m = PluginInfo{} }
func (*PluginInfo) ProtoMessage()               {}
func (*PluginInfo) Descriptor() ([]byte, []int) { return fileDescriptorApi, []int{0} }

func (m *PluginInfo) GetType() string {
	if m != nil {
		return m.Type
	}
	return ""
}

func (m *PluginInfo) GetName() string {
	if m != nil {
		return m.Name
	}
	return ""
}

func (m *PluginInfo) GetEndpoint() string {
	if m != nil {
		return m.Endpoint
	}
	return ""
}

func (m *PluginInfo) GetSupportedVersions() []string {
	if m != nil {
		return m.SupportedVersions
	}
	return nil
}

// RegistrationStatus is the message sent from Kubelet pluginwatcher to the plugin for notification on registration status
type RegistrationStatus struct {
	// True if plugin gets registered successfully at Kubelet
	PluginRegistered bool `protobuf:"varint,1,opt,name=plugin_registered,json=pluginRegistered,proto3" json:"plugin_registered,omitempty"`
	// Error message in case plugin fails to register, empty string otherwise
	Error string `protobuf:"bytes,2,opt,name=error,proto3" json:"error,omitempty"`
}

func (m *RegistrationStatus) Reset()                    { *m = RegistrationStatus{} }
func (*RegistrationStatus) ProtoMessage()               {}
func (*RegistrationStatus) Descriptor() ([]byte, []int) { return fileDescriptorApi, []int{1} }

func (m *RegistrationStatus) GetPluginRegistered() bool {
	if m != nil {
		return m.PluginRegistered
	}
	return false
}

func (m *RegistrationStatus) GetError() string {
	if m != nil {
		return m.Error
	}
	return ""
}

// RegistrationStatusResponse is sent by plugin to kubelet in response to RegistrationStatus RPC
type RegistrationStatusResponse struct {
}

func (m *RegistrationStatusResponse) Reset()                    { *m = RegistrationStatusResponse{} }
func (*RegistrationStatusResponse) ProtoMessage()               {}
func (*RegistrationStatusResponse) Descriptor() ([]byte, []int) { return fileDescriptorApi, []int{2} }

// InfoRequest is the empty request message from Kubelet
type InfoRequest struct {
}

func (m *InfoRequest) Reset()                    { *m = InfoRequest{} }
func (*InfoRequest) ProtoMessage()               {}
func (*InfoRequest) Descriptor() ([]byte, []int) { return fileDescriptorApi, []int{3} }

func init() {
	proto.RegisterType((*PluginInfo)(nil), "pluginregistration.PluginInfo")
	proto.RegisterType((*RegistrationStatus)(nil), "pluginregistration.RegistrationStatus")
	proto.RegisterType((*RegistrationStatusResponse)(nil), "pluginregistration.RegistrationStatusResponse")
	proto.RegisterType((*InfoRequest)(nil), "pluginregistration.InfoRequest")
}

// Reference imports to suppress errors if they are not otherwise used.
var _ context.Context
var _ grpc.ClientConn

// This is a compile-time assertion to ensure that this generated file
// is compatible with the grpc package it is being compiled against.
const _ = grpc.SupportPackageIsVersion4

// Client API for Registration service

type RegistrationClient interface {
	GetInfo(ctx context.Context, in *InfoRequest, opts ...grpc.CallOption) (*PluginInfo, error)
	NotifyRegistrationStatus(ctx context.Context, in *RegistrationStatus, opts ...grpc.CallOption) (*RegistrationStatusResponse, error)
}

type registrationClient struct {
	cc *grpc.ClientConn
}

func NewRegistrationClient(cc *grpc.ClientConn) RegistrationClient {
	return &registrationClient{cc}
}

func (c *registrationClient) GetInfo(ctx context.Context, in *InfoRequest, opts ...grpc.CallOption) (*PluginInfo, error) {
	out := new(PluginInfo)
	err := grpc.Invoke(ctx, "/pluginregistration.Registration/GetInfo", in, out, c.cc, opts...)
	if err != nil {
		return nil, err
	}
	return out, nil
}

func (c *registrationClient) NotifyRegistrationStatus(ctx context.Context, in *RegistrationStatus, opts ...grpc.CallOption) (*RegistrationStatusResponse, error) {
	out := new(RegistrationStatusResponse)
	err := grpc.Invoke(ctx, "/pluginregistration.Registration/NotifyRegistrationStatus", in, out, c.cc, opts...)
	if err != nil {
		return nil, err
	}
	return out, nil
}

// Server API for Registration service

type RegistrationServer interface {
	GetInfo(context.Context, *InfoRequest) (*PluginInfo, error)
	NotifyRegistrationStatus(context.Context, *RegistrationStatus) (*RegistrationStatusResponse, error)
}

func RegisterRegistrationServer(s *grpc.Server, srv RegistrationServer) {
	s.RegisterService(&_Registration_serviceDesc, srv)
}

func _Registration_GetInfo_Handler(srv interface{}, ctx context.Context, dec func(interface{}) error, interceptor grpc.UnaryServerInterceptor) (interface{}, error) {
	in := new(InfoRequest)
	if err := dec(in); err != nil {
		return nil, err
	}
	if interceptor == nil {
		return srv.(RegistrationServer).GetInfo(ctx, in)
	}
	info := &grpc.UnaryServerInfo{
		Server:     srv,
		FullMethod: "/pluginregistration.Registration/GetInfo",
	}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return srv.(RegistrationServer).GetInfo(ctx, req.(*InfoRequest))
	}
	return interceptor(ctx, in, info, handler)
}

func _Registration_NotifyRegistrationStatus_Handler(srv interface{}, ctx context.Context, dec func(interface{}) error, interceptor grpc.UnaryServerInterceptor) (interface{}, error) {
	in := new(RegistrationStatus)
	if err := dec(in); err != nil {
		return nil, err
	}
	if interceptor == nil {
		return srv.(RegistrationServer).NotifyRegistrationStatus(ctx, in)
	}
	info := &grpc.UnaryServerInfo{
		Server:     srv,
		FullMethod: "/pluginregistration.Registration/NotifyRegistrationStatus",
	}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return srv.(RegistrationServer).NotifyRegistrationStatus(ctx, req.(*RegistrationStatus))
	}
	return interceptor(ctx, in, info, handler)
}

var _Registration_serviceDesc = grpc.ServiceDesc{
	ServiceName: "pluginregistration.Registration",
	HandlerType: (*RegistrationServer)(nil),
	Methods: []grpc.MethodDesc{
		{
			MethodName: "GetInfo",
			Handler:    _Registration_GetInfo_Handler,
		},
		{
			MethodName: "NotifyRegistrationStatus",
			Handler:    _Registration_NotifyRegistrationStatus_Handler,
		},
	},
	Streams:  []grpc.StreamDesc{},
	Metadata: "api.proto",
}

func (m *PluginInfo) Marshal() (dAtA []byte, err error) {
	size := m.Size()
	dAtA = make([]byte, size)
	n, err := m.MarshalTo(dAtA)
	if err != nil {
		return nil, err
	}
	return dAtA[:n], nil
}

func (m *PluginInfo) MarshalTo(dAtA []byte) (int, error) {
	var i int
	_ = i
	var l int
	_ = l
	if len(m.Type) > 0 {
		dAtA[i] = 0xa
		i++
		i = encodeVarintApi(dAtA, i, uint64(len(m.Type)))
		i += copy(dAtA[i:], m.Type)
	}
	if len(m.Name) > 0 {
		dAtA[i] = 0x12
		i++
		i = encodeVarintApi(dAtA, i, uint64(len(m.Name)))
		i += copy(dAtA[i:], m.Name)
	}
	if len(m.Endpoint) > 0 {
		dAtA[i] = 0x1a
		i++
		i = encodeVarintApi(dAtA, i, uint64(len(m.Endpoint)))
		i += copy(dAtA[i:], m.Endpoint)
	}
	if len(m.SupportedVersions) > 0 {
		for _, s := range m.SupportedVersions {
			dAtA[i] = 0x22
			i++
			l = len(s)
			for l >= 1<<7 {
				dAtA[i] = uint8(uint64(l)&0x7f | 0x80)
				l >>= 7
				i++
			}
			dAtA[i] = uint8(l)
			i++
			i += copy(dAtA[i:], s)
		}
	}
	return i, nil
}

func (m *RegistrationStatus) Marshal() (dAtA []byte, err error) {
	size := m.Size()
	dAtA = make([]byte, size)
	n, err := m.MarshalTo(dAtA)
	if err != nil {
		return nil, err
	}
	return dAtA[:n], nil
}

func (m *RegistrationStatus) MarshalTo(dAtA []byte) (int, error) {
	var i int
	_ = i
	var l int
	_ = l
	if m.PluginRegistered {
		dAtA[i] = 0x8
		i++
		if m.PluginRegistered {
			dAtA[i] = 1
		} else {
			dAtA[i] = 0
		}
		i++
	}
	if len(m.Error) > 0 {
		dAtA[i] = 0x12
		i++
		i = encodeVarintApi(dAtA, i, uint64(len(m.Error)))
		i += copy(dAtA[i:], m.Error)
	}
	return i, nil
}

func (m *RegistrationStatusResponse) Marshal() (dAtA []byte, err error) {
	size := m.Size()
	dAtA = make([]byte, size)
	n, err := m.MarshalTo(dAtA)
	if err != nil {
		return nil, err
	}
	return dAtA[:n], nil
}

func (m *RegistrationStatusResponse) MarshalTo(dAtA []byte) (int, error) {
	var i int
	_ = i
	var l int
	_ = l
	return i, nil
}

func (m *InfoRequest) Marshal() (dAtA []byte, err error) {
	size := m.Size()
	dAtA = make([]byte, size)
	n, err := m.MarshalTo(dAtA)
	if err != nil {
		return nil, err
	}
	return dAtA[:n], nil
}

func (m *InfoRequest) MarshalTo(dAtA []byte) (int, error) {
	var i int
	_ = i
	var l int
	_ = l
	return i, nil
}

func encodeVarintApi(dAtA []byte, offset int, v uint64) int {
	for v >= 1<<7 {
		dAtA[offset] = uint8(v&0x7f | 0x80)
		v >>= 7
		offset++
	}
	dAtA[offset] = uint8(v)
	return offset + 1
}
func (m *PluginInfo) Size() (n int) {
	var l int
	_ = l
	l = len(m.Type)
	if l > 0 {
		n += 1 + l + sovApi(uint64(l))
	}
	l = len(m.Name)
	if l > 0 {
		n += 1 + l + sovApi(uint64(l))
	}
	l = len(m.Endpoint)
	if l > 0 {
		n += 1 + l + sovApi(uint64(l))
	}
	if len(m.SupportedVersions) > 0 {
		for _, s := range m.SupportedVersions {
			l = len(s)
			n += 1 + l + sovApi(uint64(l))
		}
	}
	return n
}

func (m *RegistrationStatus) Size() (n int) {
	var l int
	_ = l
	if m.PluginRegistered {
		n += 2
	}
	l = len(m.Error)
	if l > 0 {
		n += 1 + l + sovApi(uint64(l))
	}
	return n
}

func (m *RegistrationStatusResponse) Size() (n int) {
	var l int
	_ = l
	return n
}

func (m *InfoRequest) Size() (n int) {
	var l int
	_ = l
	return n
}

func sovApi(x uint64) (n int) {
	for {
		n++
		x >>= 7
		if x == 0 {
			break
		}
	}
	return n
}
func sozApi(x uint64) (n int) {
	return sovApi(uint64((x << 1) ^ uint64((int64(x) >> 63))))
}
func (this *PluginInfo) String() string {
	if this == nil {
		return "nil"
	}
	s := strings.Join([]string{`&PluginInfo{`,
		`Type:` + fmt.Sprintf("%v", this.Type) + `,`,
		`Name:` + fmt.Sprintf("%v", this.Name) + `,`,
		`Endpoint:` + fmt.Sprintf("%v", this.Endpoint) + `,`,
		`SupportedVersions:` + fmt.Sprintf("%v", this.SupportedVersions) + `,`,
		`}`,
	}, "")
	return s
}
func (this *RegistrationStatus) String() string {
	if this == nil {
		return "nil"
	}
	s := strings.Join([]string{`&RegistrationStatus{`,
		`PluginRegistered:` + fmt.Sprintf("%v", this.PluginRegistered) + `,`,
		`Error:` + fmt.Sprintf("%v", this.Error) + `,`,
		`}`,
	}, "")
	return s
}
func (this *RegistrationStatusResponse) String() string {
	if this == nil {
		return "nil"
	}
	s := strings.Join([]string{`&RegistrationStatusResponse{`,
		`}`,
	}, "")
	return s
}
func (this *InfoRequest) String() string {
	if this == nil {
		return "nil"
	}
	s := strings.Join([]string{`&InfoRequest{`,
		`}`,
	}, "")
	return s
}
func valueToStringApi(v interface{}) string {
	rv := reflect.ValueOf(v)
	if rv.IsNil() {
		return "nil"
	}
	pv := reflect.Indirect(rv).Interface()
	return fmt.Sprintf("*%v", pv)
}
func (m *PluginInfo) Unmarshal(dAtA []byte) error {
	l := len(dAtA)
	iNdEx := 0
	for iNdEx < l {
		preIndex := iNdEx
		var wire uint64
		for shift := uint(0); ; shift += 7 {
			if shift >= 64 {
				return ErrIntOverflowApi
			}
			if iNdEx >= l {
				return io.ErrUnexpectedEOF
			}
			b := dAtA[iNdEx]
			iNdEx++
			wire |= (uint64(b) & 0x7F) << shift
			if b < 0x80 {
				break
			}
		}
		fieldNum := int32(wire >> 3)
		wireType := int(wire & 0x7)
		if wireType == 4 {
			return fmt.Errorf("proto: PluginInfo: wiretype end group for non-group")
		}
		if fieldNum <= 0 {
			return fmt.Errorf("proto: PluginInfo: illegal tag %d (wire type %d)", fieldNum, wire)
		}
		switch fieldNum {
		case 1:
			if wireType != 2 {
				return fmt.Errorf("proto: wrong wireType = %d for field Type", wireType)
			}
			var stringLen uint64
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return ErrIntOverflowApi
				}
				if iNdEx >= l {
					return io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				stringLen |= (uint64(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			intStringLen := int(stringLen)
			if intStringLen < 0 {
				return ErrInvalidLengthApi
			}
			postIndex := iNdEx + intStringLen
			if postIndex > l {
				return io.ErrUnexpectedEOF
			}
			m.Type = string(dAtA[iNdEx:postIndex])
			iNdEx = postIndex
		case 2:
			if wireType != 2 {
				return fmt.Errorf("proto: wrong wireType = %d for field Name", wireType)
			}
			var stringLen uint64
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return ErrIntOverflowApi
				}
				if iNdEx >= l {
					return io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				stringLen |= (uint64(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			intStringLen := int(stringLen)
			if intStringLen < 0 {
				return ErrInvalidLengthApi
			}
			postIndex := iNdEx + intStringLen
			if postIndex > l {
				return io.ErrUnexpectedEOF
			}
			m.Name = string(dAtA[iNdEx:postIndex])
			iNdEx = postIndex
		case 3:
			if wireType != 2 {
				return fmt.Errorf("proto: wrong wireType = %d for field Endpoint", wireType)
			}
			var stringLen uint64
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return ErrIntOverflowApi
				}
				if iNdEx >= l {
					return io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				stringLen |= (uint64(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			intStringLen := int(stringLen)
			if intStringLen < 0 {
				return ErrInvalidLengthApi
			}
			postIndex := iNdEx + intStringLen
			if postIndex > l {
				return io.ErrUnexpectedEOF
			}
			m.Endpoint = string(dAtA[iNdEx:postIndex])
			iNdEx = postIndex
		case 4:
			if wireType != 2 {
				return fmt.Errorf("proto: wrong wireType = %d for field SupportedVersions", wireType)
			}
			var stringLen uint64
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return ErrIntOverflowApi
				}
				if iNdEx >= l {
					return io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				stringLen |= (uint64(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			intStringLen := int(stringLen)
			if intStringLen < 0 {
				return ErrInvalidLengthApi
			}
			postIndex := iNdEx + intStringLen
			if postIndex > l {
				return io.ErrUnexpectedEOF
			}
			m.SupportedVersions = append(m.SupportedVersions, string(dAtA[iNdEx:postIndex]))
			iNdEx = postIndex
		default:
			iNdEx = preIndex
			skippy, err := skipApi(dAtA[iNdEx:])
			if err != nil {
				return err
			}
			if skippy < 0 {
				return ErrInvalidLengthApi
			}
			if (iNdEx + skippy) > l {
				return io.ErrUnexpectedEOF
			}
			iNdEx += skippy
		}
	}

	if iNdEx > l {
		return io.ErrUnexpectedEOF
	}
	return nil
}
func (m *RegistrationStatus) Unmarshal(dAtA []byte) error {
	l := len(dAtA)
	iNdEx := 0
	for iNdEx < l {
		preIndex := iNdEx
		var wire uint64
		for shift := uint(0); ; shift += 7 {
			if shift >= 64 {
				return ErrIntOverflowApi
			}
			if iNdEx >= l {
				return io.ErrUnexpectedEOF
			}
			b := dAtA[iNdEx]
			iNdEx++
			wire |= (uint64(b) & 0x7F) << shift
			if b < 0x80 {
				break
			}
		}
		fieldNum := int32(wire >> 3)
		wireType := int(wire & 0x7)
		if wireType == 4 {
			return fmt.Errorf("proto: RegistrationStatus: wiretype end group for non-group")
		}
		if fieldNum <= 0 {
			return fmt.Errorf("proto: RegistrationStatus: illegal tag %d (wire type %d)", fieldNum, wire)
		}
		switch fieldNum {
		case 1:
			if wireType != 0 {
				return fmt.Errorf("proto: wrong wireType = %d for field PluginRegistered", wireType)
			}
			var v int
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return ErrIntOverflowApi
				}
				if iNdEx >= l {
					return io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				v |= (int(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			m.PluginRegistered = bool(v != 0)
		case 2:
			if wireType != 2 {
				return fmt.Errorf("proto: wrong wireType = %d for field Error", wireType)
			}
			var stringLen uint64
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return ErrIntOverflowApi
				}
				if iNdEx >= l {
					return io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				stringLen |= (uint64(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			intStringLen := int(stringLen)
			if intStringLen < 0 {
				return ErrInvalidLengthApi
			}
			postIndex := iNdEx + intStringLen
			if postIndex > l {
				return io.ErrUnexpectedEOF
			}
			m.Error = string(dAtA[iNdEx:postIndex])
			iNdEx = postIndex
		default:
			iNdEx = preIndex
			skippy, err := skipApi(dAtA[iNdEx:])
			if err != nil {
				return err
			}
			if skippy < 0 {
				return ErrInvalidLengthApi
			}
			if (iNdEx + skippy) > l {
				return io.ErrUnexpectedEOF
			}
			iNdEx += skippy
		}
	}

	if iNdEx > l {
		return io.ErrUnexpectedEOF
	}
	return nil
}
func (m *RegistrationStatusResponse) Unmarshal(dAtA []byte) error {
	l := len(dAtA)
	iNdEx := 0
	for iNdEx < l {
		preIndex := iNdEx
		var wire uint64
		for shift := uint(0); ; shift += 7 {
			if shift >= 64 {
				return ErrIntOverflowApi
			}
			if iNdEx >= l {
				return io.ErrUnexpectedEOF
			}
			b := dAtA[iNdEx]
			iNdEx++
			wire |= (uint64(b) & 0x7F) << shift
			if b < 0x80 {
				break
			}
		}
		fieldNum := int32(wire >> 3)
		wireType := int(wire & 0x7)
		if wireType == 4 {
			return fmt.Errorf("proto: RegistrationStatusResponse: wiretype end group for non-group")
		}
		if fieldNum <= 0 {
			return fmt.Errorf("proto: RegistrationStatusResponse: illegal tag %d (wire type %d)", fieldNum, wire)
		}
		switch fieldNum {
		default:
			iNdEx = preIndex
			skippy, err := skipApi(dAtA[iNdEx:])
			if err != nil {
				return err
			}
			if skippy < 0 {
				return ErrInvalidLengthApi
			}
			if (iNdEx + skippy) > l {
				return io.ErrUnexpectedEOF
			}
			iNdEx += skippy
		}
	}

	if iNdEx > l {
		return io.ErrUnexpectedEOF
	}
	return nil
}
func (m *InfoRequest) Unmarshal(dAtA []byte) error {
	l := len(dAtA)
	iNdEx := 0
	for iNdEx < l {
		preIndex := iNdEx
		var wire uint64
		for shift := uint(0); ; shift += 7 {
			if shift >= 64 {
				return ErrIntOverflowApi
			}
			if iNdEx >= l {
				return io.ErrUnexpectedEOF
			}
			b := dAtA[iNdEx]
			iNdEx++
			wire |= (uint64(b) & 0x7F) << shift
			if b < 0x80 {
				break
			}
		}
		fieldNum := int32(wire >> 3)
		wireType := int(wire & 0x7)
		if wireType == 4 {
			return fmt.Errorf("proto: InfoRequest: wiretype end group for non-group")
		}
		if fieldNum <= 0 {
			return fmt.Errorf("proto: InfoRequest: illegal tag %d (wire type %d)", fieldNum, wire)
		}
		switch fieldNum {
		default:
			iNdEx = preIndex
			skippy, err := skipApi(dAtA[iNdEx:])
			if err != nil {
				return err
			}
			if skippy < 0 {
				return ErrInvalidLengthApi
			}
			if (iNdEx + skippy) > l {
				return io.ErrUnexpectedEOF
			}
			iNdEx += skippy
		}
	}

	if iNdEx > l {
		return io.ErrUnexpectedEOF
	}
	return nil
}
func skipApi(dAtA []byte) (n int, err error) {
	l := len(dAtA)
	iNdEx := 0
	for iNdEx < l {
		var wire uint64
		for shift := uint(0); ; shift += 7 {
			if shift >= 64 {
				return 0, ErrIntOverflowApi
			}
			if iNdEx >= l {
				return 0, io.ErrUnexpectedEOF
			}
			b := dAtA[iNdEx]
			iNdEx++
			wire |= (uint64(b) & 0x7F) << shift
			if b < 0x80 {
				break
			}
		}
		wireType := int(wire & 0x7)
		switch wireType {
		case 0:
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return 0, ErrIntOverflowApi
				}
				if iNdEx >= l {
					return 0, io.ErrUnexpectedEOF
				}
				iNdEx++
				if dAtA[iNdEx-1] < 0x80 {
					break
				}
			}
			return iNdEx, nil
		case 1:
			iNdEx += 8
			return iNdEx, nil
		case 2:
			var length int
			for shift := uint(0); ; shift += 7 {
				if shift >= 64 {
					return 0, ErrIntOverflowApi
				}
				if iNdEx >= l {
					return 0, io.ErrUnexpectedEOF
				}
				b := dAtA[iNdEx]
				iNdEx++
				length |= (int(b) & 0x7F) << shift
				if b < 0x80 {
					break
				}
			}
			iNdEx += length
			if length < 0 {
				return 0, ErrInvalidLengthApi
			}
			return iNdEx, nil
		case 3:
			for {
				var innerWire uint64
				var start int = iNdEx
				for shift := uint(0); ; shift += 7 {
					if shift >= 64 {
						return 0, ErrIntOverflowApi
					}
					if iNdEx >= l {
						return 0, io.ErrUnexpectedEOF
					}
					b := dAtA[iNdEx]
					iNdEx++
					innerWire |= (uint64(b) & 0x7F) << shift
					if b < 0x80 {
						break
					}
				}
				innerWireType := int(innerWire & 0x7)
				if innerWireType == 4 {
					break
				}
				next, err := skipApi(dAtA[start:])
				if err != nil {
					return 0, err
				}
				iNdEx = start + next
			}
			return iNdEx, nil
		case 4:
			return iNdEx, nil
		case 5:
			iNdEx += 4
			return iNdEx, nil
		default:
			return 0, fmt.Errorf("proto: illegal wireType %d", wireType)
		}
	}
	panic("unreachable")
}

var (
	ErrInvalidLengthApi = fmt.Errorf("proto: negative length found during unmarshaling")
	ErrIntOverflowApi   = fmt.Errorf("proto: integer overflow")
)

func init() { proto.RegisterFile("api.proto", fileDescriptorApi) }

var fileDescriptorApi = []byte{
	// 337 bytes of a gzipped FileDescriptorProto
	0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x8c, 0x52, 0x41, 0x4b, 0x33, 0x31,
	0x14, 0xdc, 0x7c, 0xed, 0xa7, 0xed, 0x53, 0xc1, 0x06, 0x0f, 0xcb, 0x52, 0x62, 0xd9, 0x83, 0x14,
	0xa4, 0x5b, 0xd0, 0x7f, 0xe0, 0x45, 0x04, 0x11, 0x89, 0xa0, 0xc7, 0xb2, 0xb5, 0xaf, 0x6b, 0xc0,
	0x26, 0x31, 0xc9, 0x0a, 0x3d, 0xe9, 0x4f, 0xf0, 0x67, 0xf5, 0x28, 0x9e, 0x3c, 0xda, 0xf5, 0x8f,
	0x48, 0xb3, 0x65, 0x2d, 0xb4, 0x07, 0x6f, 0x6f, 0xe6, 0x4d, 0x1e, 0x33, 0x43, 0xa0, 0x99, 0x6a,
	0x91, 0x68, 0xa3, 0x9c, 0xa2, 0x54, 0x3f, 0xe6, 0x99, 0x90, 0x06, 0x33, 0x61, 0x9d, 0x49, 0x9d,
	0x50, 0x32, 0xea, 0x65, 0xc2, 0x3d, 0xe4, 0xc3, 0xe4, 0x5e, 0x4d, 0xfa, 0x99, 0xca, 0x54, 0xdf,
	0x4b, 0x87, 0xf9, 0xd8, 0x23, 0x0f, 0xfc, 0x54, 0x9e, 0x88, 0x5f, 0x00, 0xae, 0xfd, 0x91, 0x0b,
	0x39, 0x56, 0x94, 0x42, 0xdd, 0x4d, 0x35, 0x86, 0xa4, 0x43, 0xba, 0x4d, 0xee, 0xe7, 0x05, 0x27,
	0xd3, 0x09, 0x86, 0xff, 0x4a, 0x6e, 0x31, 0xd3, 0x08, 0x1a, 0x28, 0x47, 0x5a, 0x09, 0xe9, 0xc2,
	0x9a, 0xe7, 0x2b, 0x4c, 0x7b, 0x40, 0x6d, 0xae, 0xb5, 0x32, 0x0e, 0x47, 0x83, 0x67, 0x34, 0x56,
	0x28, 0x69, 0xc3, 0x7a, 0xa7, 0xd6, 0x6d, 0xf2, 0x56, 0xb5, 0xb9, 0x5d, 0x2e, 0xe2, 0x3b, 0xa0,
	0x7c, 0xc5, 0xff, 0x8d, 0x4b, 0x5d, 0x6e, 0xe9, 0x31, 0xb4, 0xca, 0x6c, 0x83, 0x32, 0x1c, 0x1a,
	0x1c, 0x79, 0x57, 0x0d, 0xbe, 0x5f, 0x2e, 0x78, 0xc5, 0xd3, 0x03, 0xf8, 0x8f, 0xc6, 0x28, 0xb3,
	0xb4, 0x58, 0x82, 0xb8, 0x0d, 0xd1, 0xfa, 0x61, 0x8e, 0x56, 0x2b, 0x69, 0x31, 0xde, 0x83, 0x9d,
	0x45, 0x62, 0x8e, 0x4f, 0x39, 0x5a, 0x77, 0xf2, 0x41, 0x60, 0x77, 0x55, 0x4d, 0x2f, 0x61, 0xfb,
	0x1c, 0x9d, 0x2f, 0xe5, 0x30, 0x59, 0xaf, 0x39, 0x59, 0x79, 0x1c, 0xb1, 0x4d, 0x82, 0xdf, 0x56,
	0xe3, 0x80, 0x3a, 0x08, 0xaf, 0x94, 0x13, 0xe3, 0xe9, 0x86, 0xa8, 0x47, 0x9b, 0x5e, 0xaf, 0xeb,
	0xa2, 0xe4, 0x6f, 0xba, 0x2a, 0x61, 0x70, 0xd6, 0x9e, 0xcd, 0x19, 0xf9, 0x9c, 0xb3, 0xe0, 0xb5,
	0x60, 0x64, 0x56, 0x30, 0xf2, 0x5e, 0x30, 0xf2, 0x55, 0x30, 0xf2, 0xf6, 0xcd, 0x82, 0xe1, 0x96,
	0xff, 0x00, 0xa7, 0x3f, 0x01, 0x00, 0x00, 0xff, 0xff, 0xe4, 0xc0, 0xe3, 0x42, 0x50, 0x02, 0x00,
	0x00,
}
