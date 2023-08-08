/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include <memory>

#include "mongo/db/exec/sbe/values/block_interface.h"
#include "mongo/db/exec/sbe/values/value.h"

namespace mongo::sbe::value {
/**
 * CellBlock corresponds to a path for a given document and is a container of values at a "path"
 * according to the chosen definition of "path".
 *
 * For example, a TS bucket has the "data" field that stores the actual data in a columnar format.
 * Each top-level field in the "data" field could be a CellBlock, in which case the "path"
 * definition is the top-level field name.
 */
struct CellBlock {
    virtual ~CellBlock() = default;

    /**
     * Returns the value block for this cell block. The value block is the block of values that
     * corresponds to the path of this cell block.
     */
    virtual ValueBlock& getValueBlock() = 0;

    /**
     * Makes a fully independent clone of this CellBlock.
     */
    virtual std::unique_ptr<CellBlock> clone() const = 0;
};
}  // namespace mongo::sbe::value
