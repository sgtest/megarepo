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

#include <cstddef>

#include "mongo/bson/bsonobj.h"
#include "mongo/db/operation_context.h"
#include "mongo/util/decorable.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/time_support.h"

namespace mongo {

/**
 * The state associated with tailable cursors.
 */
struct AwaitDataState {
    /**
     * The deadline for how long we wait on the tail of capped collection before returning IS_EOF.
     */
    Date_t waitForInsertsDeadline;

    /**
     * If true, when no results are available from a plan, then instead of returning immediately,
     * the system should wait up to the length of the operation deadline for data to be inserted
     * which causes results to become available.
     */
    bool shouldWaitForInserts;
};

extern const OperationContext::Decoration<AwaitDataState> awaitDataState;

class BSONObj;
class CanonicalQuery;
class FindCommandRequest;

// Failpoint for making find hang.
extern FailPoint waitInFindBeforeMakingBatch;

// Failpoint for making getMore not wait for an awaitdata cursor. Allows us to avoid waiting during
// tests.
extern FailPoint disableAwaitDataForGetMoreCmd;

// Enabling this fail point will cause getMores to busy wait after pinning the cursor
// but before we have started building the batch, until the fail point is disabled.
extern FailPoint waitAfterPinningCursorBeforeGetMoreBatch;

// Enabling this fail point will cause getMores to busy wait after setting up the plan executor and
// before beginning the batch.
extern FailPoint waitWithPinnedCursorDuringGetMoreBatch;

// Enabling this failpoint will cause getMores to wait just before it unpins its cursor after it
// has completed building the current batch.
extern FailPoint waitBeforeUnpinningOrDeletingCursorAfterGetMoreBatch;

// Enabling this failpoint will cause a getMore to fail with a specified exception after it has
// checked out its cursor and the originating command has been made available to CurOp.
extern FailPoint failGetMoreAfterCursorCheckout;

/**
 * Suite of find/getMore related functions used in both the mongod and mongos query paths.
 */
class FindCommon {
public:
    // The maximum amount of user data to return to a client in a single batch.
    //
    // This max may be exceeded by epsilon for output documents that approach the maximum user
    // document size. That is, if we must return a BSONObjMaxUserSize document, then the total
    // response size will be BSONObjMaxUserSize plus the amount of size required for the message
    // header and the cursor response "envelope". (The envelope contains namespace and cursor id
    // info.)
    static const size_t kMaxBytesToReturnToClientAtOnce;

    // The estimated amount of user data in a GetMore command response for a tailable cursor.
    // This amount will be used for memory pre-allocation in this type of requests.
    // Note: as this is an estimate, we request 1KB less than a full power of two, so that the
    // memory allocator will not jump to the next power of two once the envelope size is added in.
    static const size_t kTailableGetMoreReplyBufferSize;

    // The minimum document size we are prepared to consider when preallocating a reply buffer for
    // getMore requests. We use a combination of the batchSize and the the actual size of the first
    // document in the batch to compute the amount of bytes to preallocate. If the document size is
    // below this threshold, we calculate the reply buffer using this constant in order to avoid
    // under-allocating and having to grow the buffer again later.
    static const size_t kMinDocSizeForGetMorePreAllocation;

    // The initial size of the query response buffer.
    static const size_t kInitReplyBufferSize;

    /**
     * Returns true if the batchSize for the initial find has been satisfied.
     *
     * If 'qr' does not have a batchSize, the default batchSize is respected.
     */
    static bool enoughForFirstBatch(const FindCommandRequest& findCommand, long long numDocs);

    /**
     * Returns true if the batchSize for the getMore has been satisfied.
     *
     * An 'effectiveBatchSize' value of zero is interpreted as the absence of a batchSize, in which
     * case this method returns false.
     */
    static bool enoughForGetMore(long long effectiveBatchSize, long long numDocs) {
        return effectiveBatchSize && numDocs >= effectiveBatchSize;
    }

    /**
     * Given the number of docs ('numDocs') and bytes ('bytesBuffered') currently buffered as a
     * response to a cursor-generating command, returns true if there are enough remaining bytes in
     * our budget to fit 'nextDoc'.
     */
    static bool haveSpaceForNext(const BSONObj& nextDoc, long long numDocs, size_t bytesBuffered);

    /**
     * This function wraps waitWhileFailPointEnabled() on waitInFindBeforeMakingBatch.
     *
     * Since query processing happens in three different places, this function makes it easier to
     * check the failpoint for a query's namespace and log a helpful diagnostic message when the
     * failpoint is active.
     */
    static void waitInFindBeforeMakingBatch(OperationContext* opCtx, const CanonicalQuery& cq);

    /**
     * Computes an initial preallocation size for the GetMore reply buffer based on its properties.
     */
    static std::size_t getBytesToReserveForGetMoreReply(bool isTailable,
                                                        size_t firstResultSize,
                                                        size_t batchSize);

    /**
     * Tracker of a size of a server response presented as a BSON array. Facilitates limiting the
     * server response size to 16MB + certain epsilon. Accounts for array element and it's overhead
     * size. Does not account for response "envelope" size.
     */
    class BSONArrayResponseSizeTracker {
        // Upper bound of BSON array element overhead.
        static const size_t kPerDocumentOverheadBytesUpperBound;

    public:
        /**
         * Returns true only if 'document' can be added to the BSON array without violating the
         * overall response size limit or if it is the first document.
         */
        bool haveSpaceForNext(const BSONObj& document);

        /**
         * Records that 'document' was added to the response.
         */
        void add(const BSONObj& document);

    private:
        std::size_t _numberOfDocuments{0};
        std::size_t _bsonArraySizeInBytes{0};
    };
};

}  // namespace mongo
