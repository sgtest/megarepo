/**
 *    Copyright (C) 2024-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#pragma once

#include <boost/smart_ptr/allocate_unique.hpp>
#include <memory>
#include <scoped_allocator>
#include <vector>

#include "mongo/db/timeseries/timeseries_tracking_allocator.h"
#include "mongo/db/timeseries/timeseries_tracking_context.h"
#include "mongo/stdx/unordered_map.h"

namespace mongo::timeseries {

template <class T>
using shared_tracked_ptr = std::shared_ptr<T>;

template <class T, class... Args>
shared_tracked_ptr<T> make_shared_tracked(TrackingContext& trackingContext, Args... args) {
    return std::allocate_shared<T>(trackingContext.makeAllocator<T>(), args...);
}

template <class T>
using unique_tracked_ptr = std::unique_ptr<T, boost::alloc_deleter<T, TrackingAllocator<T>>>;

template <class T, class... Args>
unique_tracked_ptr<T> make_unique_tracked(TrackingContext& trackingContext, Args... args) {
    return boost::allocate_unique<T>(trackingContext.makeAllocator<T>(), args...);
}

template <class Key, class T, class Compare = std::less<Key>>
using tracked_map =
    std::map<Key,
             T,
             Compare,
             std::scoped_allocator_adaptor<timeseries::TrackingAllocator<std::pair<const Key, T>>>>;

template <class Key, class T, class Compare = std::less<Key>>
tracked_map<Key, T, Compare> make_tracked_map(TrackingContext& trackingContext) {
    return tracked_map<Key, T, Compare>(trackingContext.makeAllocator<T>());
}

template <class Key,
          class Value,
          class Hasher = DefaultHasher<Key>,
          class KeyEqual = std::equal_to<Key>>
using tracked_unordered_map = stdx::unordered_map<
    Key,
    Value,
    Hasher,
    KeyEqual,
    std::scoped_allocator_adaptor<timeseries::TrackingAllocator<std::pair<const Key, Value>>>>;

template <class Key,
          class Value,
          class Hasher = DefaultHasher<Key>,
          class KeyEqual = std::equal_to<Key>>
tracked_unordered_map<Key, Value, Hasher> make_tracked_unordered_map(
    TrackingContext& trackingContext) {
    return tracked_unordered_map<Key, Value, Hasher, KeyEqual>(
        trackingContext.makeAllocator<Value>());
}

using tracked_string =
    std::basic_string<char, std::char_traits<char>, timeseries::TrackingAllocator<char>>;

template <class... Args>
tracked_string make_tracked_string(TrackingContext& trackingContext, Args... args) {
    return tracked_string(args..., trackingContext.makeAllocator<char>());
}

template <class T>
using tracked_vector =
    std::vector<T, std::scoped_allocator_adaptor<timeseries::TrackingAllocator<T>>>;

template <class T, class... Args>
tracked_vector<T> make_tracked_vector(TrackingContext& trackingContext, Args... args) {
    return tracked_vector<T>(args..., trackingContext.makeAllocator<T>());
}

}  // namespace mongo::timeseries
