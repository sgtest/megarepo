/* Copyright 2023 The TensorFlow Authors. All Rights Reserved.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
==============================================================================*/

#include "xla/ffi/ffi.h"

#include <cstdint>
#include <string_view>
#include <vector>

#include "absl/status/status.h"
#include "xla/ffi/call_frame.h"
#include "xla/ffi/ffi_api.h"
#include "xla/service/service_executable_run_options.h"
#include "xla/stream_executor/device_memory.h"
#include "xla/xla_data.pb.h"
#include "tsl/lib/core/status_test_util.h"
#include "tsl/platform/test.h"

namespace xla::ffi {

TEST(FfiTest, StaticRegistration) {
  static constexpr auto* noop = +[] { return absl::OkStatus(); };

  XLA_FFI_DEFINE_HANDLER(NoOp, noop, Ffi::Bind());
  XLA_FFI_REGISTER_HANDLER(GetXlaFfiApi(), "no-op", NoOp);

  auto handler = FindHandler("no-op");
  TF_ASSERT_OK(handler.status());
}

TEST(FfiTest, ForwardError) {
  auto call_frame = CallFrameBuilder().Build();
  auto handler = Ffi::Bind().To([] { return absl::AbortedError("Ooops!"); });
  auto status = Call(*handler, call_frame);
  ASSERT_EQ(status.message(), "Ooops!");
}

TEST(FfiTest, WrongNumArgs) {
  CallFrameBuilder builder;
  builder.AddBufferArg(se::DeviceMemoryBase(nullptr), PrimitiveType::F32, {});
  auto call_frame = builder.Build();

  auto handler = Ffi::Bind().Arg<Buffer>().Arg<Buffer>().To(
      [](Buffer, Buffer) { return absl::OkStatus(); });

  auto status = Call(*handler, call_frame);

  ASSERT_EQ(status.message(),
            "Wrong number of arguments: expected 2 but got 1");
}

TEST(FfiTest, WrongNumAttrs) {
  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("i32", 42);
  attrs.Insert("f32", 42.0f);

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto handler = Ffi::Bind().Attr<int32_t>("i32").To(
      [](int32_t) { return absl::OkStatus(); });

  auto status = Call(*handler, call_frame);

  ASSERT_EQ(status.message(),
            "Wrong number of attributes: expected 1 but got 2");
}

TEST(FfiTest, BuiltinAttributes) {
  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("i32", 42);
  attrs.Insert("f32", 42.0f);
  attrs.Insert("str", "foo");

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto fn = [&](int32_t i32, float f32, std::string_view str) {
    EXPECT_EQ(i32, 42);
    EXPECT_EQ(f32, 42.0f);
    EXPECT_EQ(str, "foo");
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind()
                     .Attr<int32_t>("i32")
                     .Attr<float>("f32")
                     .Attr<std::string_view>("str")
                     .To(fn);

  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

TEST(FfiTest, AttrsAsDictionary) {
  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("i32", 42);
  attrs.Insert("f32", 42.0f);
  attrs.Insert("str", "foo");

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto fn = [&](Dictionary dict) {
    EXPECT_EQ(dict.size(), 3);

    EXPECT_TRUE(dict.contains("i32"));
    EXPECT_TRUE(dict.contains("f32"));
    EXPECT_TRUE(dict.contains("str"));

    auto i32 = dict.get<int32_t>("i32");
    auto f32 = dict.get<float>("f32");
    auto str = dict.get<std::string_view>("str");

    EXPECT_TRUE(i32.has_value());
    EXPECT_TRUE(f32.has_value());
    EXPECT_TRUE(str.has_value());

    if (i32) EXPECT_EQ(*i32, 42);
    if (f32) EXPECT_EQ(*f32, 42.0f);
    if (str) EXPECT_EQ(*str, "foo");

    EXPECT_FALSE(dict.contains("i64"));
    EXPECT_FALSE(dict.get<int64_t>("i32").has_value());
    EXPECT_FALSE(dict.get<int64_t>("i64").has_value());

    return absl::OkStatus();
  };

  auto handler = Ffi::Bind().Attrs().To(fn);
  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

TEST(FfiTest, DictionaryAttr) {
  CallFrameBuilder::FlatAttributesMap dict0;
  dict0.try_emplace("i32", 42);

  CallFrameBuilder::FlatAttributesMap dict1;
  dict1.try_emplace("f32", 42.0f);

  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("dict0", dict0);
  attrs.Insert("dict1", dict1);

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto fn = [&](Dictionary dict0, Dictionary dict1) {
    EXPECT_EQ(dict0.size(), 1);
    EXPECT_EQ(dict1.size(), 1);

    EXPECT_TRUE(dict0.contains("i32"));
    EXPECT_TRUE(dict1.contains("f32"));

    auto i32 = dict0.get<int32_t>("i32");
    auto f32 = dict1.get<float>("f32");

    EXPECT_TRUE(i32.has_value());
    EXPECT_TRUE(f32.has_value());

    if (i32) EXPECT_EQ(*i32, 42);
    if (f32) EXPECT_EQ(*f32, 42.0f);

    return absl::OkStatus();
  };

  auto handler =
      Ffi::Bind().Attr<Dictionary>("dict0").Attr<Dictionary>("dict1").To(fn);

  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

struct PairOfI32AndF32 {
  int32_t i32;
  float f32;
};

XLA_FFI_REGISTER_STRUCT_ATTR_DECODING(PairOfI32AndF32,
                                      StructMember<int32_t>("i32"),
                                      StructMember<float>("f32"));

TEST(FfiTest, StructAttr) {
  CallFrameBuilder::FlatAttributesMap dict;
  dict.try_emplace("i32", 42);
  dict.try_emplace("f32", 42.0f);

  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("str", "foo");
  attrs.Insert("i32_and_f32", dict);

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto fn = [&](std::string_view str, PairOfI32AndF32 i32_and_f32) {
    EXPECT_EQ(str, "foo");
    EXPECT_EQ(i32_and_f32.i32, 42);
    EXPECT_EQ(i32_and_f32.f32, 42.0f);
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind()
                     .Attr<std::string_view>("str")
                     .Attr<PairOfI32AndF32>("i32_and_f32")
                     .To(fn);

  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

TEST(FfiTest, AttrsAsStruct) {
  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("i32", 42);
  attrs.Insert("f32", 42.0f);

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto fn = [&](PairOfI32AndF32 i32_and_f32) {
    EXPECT_EQ(i32_and_f32.i32, 42);
    EXPECT_EQ(i32_and_f32.f32, 42.0f);
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind().Attrs<PairOfI32AndF32>().To(fn);
  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

TEST(FfiTest, DecodingErrors) {
  CallFrameBuilder::AttributesBuilder attrs;
  attrs.Insert("i32", 42);
  attrs.Insert("i64", 42);
  attrs.Insert("f32", 42.0f);
  attrs.Insert("str", "foo");

  CallFrameBuilder builder;
  builder.AddAttributes(attrs.Build());
  auto call_frame = builder.Build();

  auto fn = [](int32_t, int64_t, float, std::string_view) {
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind()
                     .Attr<int32_t>("not_i32_should_fail")
                     .Attr<int64_t>("not_i64_should_fail")
                     .Attr<float>("f32")
                     .Attr<std::string_view>("not_str_should_fail")
                     .To(fn);

  auto status = Call(*handler, call_frame);

  ASSERT_EQ(
      status.message(),
      "Failed to decode all FFI handler operands (bad operands at: 0, 1, 3)");
}

TEST(FfiTest, BufferArgument) {
  std::vector<float> storage(4, 0.0f);
  se::DeviceMemoryBase memory(storage.data(), 4 * sizeof(float));

  CallFrameBuilder builder;
  builder.AddBufferArg(memory, PrimitiveType::F32, /*dims=*/{2, 2});
  auto call_frame = builder.Build();

  auto fn = [&](Buffer buffer) {
    EXPECT_EQ(buffer.dtype, PrimitiveType::F32);
    EXPECT_EQ(buffer.data.opaque(), storage.data());
    EXPECT_EQ(buffer.dimensions.size(), 2);
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind().Arg<Buffer>().To(fn);
  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

TEST(FfiTest, RemainingArgs) {
  std::vector<float> storage(4, 0.0f);
  se::DeviceMemoryBase memory(storage.data(), 4 * sizeof(float));

  CallFrameBuilder builder;
  builder.AddBufferArg(memory, PrimitiveType::F32, /*dims=*/{2, 2});
  auto call_frame = builder.Build();

  auto fn = [&](RemainingArgs args) {
    EXPECT_EQ(args.size(), 1);
    EXPECT_TRUE(args.get<Buffer>(0).has_value());
    EXPECT_FALSE(args.get<Buffer>(1).has_value());
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind().RemainingArgs().To(fn);
  auto status = Call(*handler, call_frame);

  TF_ASSERT_OK(status);
}

TEST(FfiTest, RunOptionsCtx) {
  auto call_frame = CallFrameBuilder().Build();
  auto* expected = reinterpret_cast<ServiceExecutableRunOptions*>(0x01234567);

  auto fn = [&](const ServiceExecutableRunOptions* run_options) {
    EXPECT_EQ(run_options, expected);
    return absl::OkStatus();
  };

  auto handler = Ffi::Bind().Ctx<ServiceExecutableRunOptions>().To(fn);
  auto status = Call(*handler, call_frame, {expected});

  TF_ASSERT_OK(status);
}

}  // namespace xla::ffi
