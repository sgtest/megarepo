/**
 *    Copyright (C) 2018-present MongoDB, Inc.
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

#include "mongo/transport/session.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/platform/atomic_word.h"

namespace mongo {
namespace transport {

namespace {

AtomicWord<unsigned long long> sessionIdCounter(0);

}  // namespace

Session::Session() : _id(sessionIdCounter.addAndFetch(1)), _tags(kPending) {}

void Session::setTags(TagMask tagsToSet) {
    mutateTags([tagsToSet](TagMask originalTags) { return (originalTags | tagsToSet); });
}

void Session::unsetTags(TagMask tagsToUnset) {
    mutateTags([tagsToUnset](TagMask originalTags) { return (originalTags & ~tagsToUnset); });
}

void Session::mutateTags(const std::function<TagMask(TagMask)>& mutateFunc) {
    TagMask oldValue, newValue;
    do {
        oldValue = _tags.load();
        newValue = mutateFunc(oldValue);

        // Any change to the session tags automatically clears kPending status.
        newValue &= ~kPending;
    } while (!_tags.compareAndSwap(&oldValue, newValue));
}

Session::TagMask Session::getTags() const {
    return _tags.load();
}

}  // namespace transport
}  // namespace mongo
