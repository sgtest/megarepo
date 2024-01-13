#!/bin/sh

test_description='pack-objects multi-pack reuse'

. ./test-lib.sh
. "$TEST_DIRECTORY"/lib-bitmap.sh

objdir=.git/objects
packdir=$objdir/pack

test_pack_reused () {
	test_trace2_data pack-objects pack-reused "$1"
}

test_packs_reused () {
	test_trace2_data pack-objects packs-reused "$1"
}


# pack_position <object> </path/to/pack.idx
pack_position () {
	git show-index >objects &&
	grep "$1" objects | cut -d" " -f1
}

test_expect_success 'preferred pack is reused for single-pack reuse' '
	test_config pack.allowPackReuse single &&

	for i in A B
	do
		test_commit "$i" &&
		git repack -d || return 1
	done &&

	git multi-pack-index write --bitmap &&

	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --revs --all >/dev/null &&

	test_pack_reused 3 <trace2.txt &&
	test_packs_reused 1 <trace2.txt
'

test_expect_success 'enable multi-pack reuse' '
	git config pack.allowPackReuse multi
'

test_expect_success 'reuse all objects from subset of bitmapped packs' '
	test_commit C &&
	git repack -d &&

	git multi-pack-index write --bitmap &&

	cat >in <<-EOF &&
	$(git rev-parse C)
	^$(git rev-parse A)
	EOF

	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --revs <in >/dev/null &&

	test_pack_reused 6 <trace2.txt &&
	test_packs_reused 2 <trace2.txt
'

test_expect_success 'reuse all objects from all packs' '
	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --revs --all >/dev/null &&

	test_pack_reused 9 <trace2.txt &&
	test_packs_reused 3 <trace2.txt
'

test_expect_success 'reuse objects from first pack with middle gap' '
	for i in D E F
	do
		test_commit "$i" || return 1
	done &&

	# Set "pack.window" to zero to ensure that we do not create any
	# deltas, which could alter the amount of pack reuse we perform
	# (if, for e.g., we are not sending one or more bases).
	D="$(git -c pack.window=0 pack-objects --all --unpacked $packdir/pack)" &&

	d_pos="$(pack_position $(git rev-parse D) <$packdir/pack-$D.idx)" &&
	e_pos="$(pack_position $(git rev-parse E) <$packdir/pack-$D.idx)" &&
	f_pos="$(pack_position $(git rev-parse F) <$packdir/pack-$D.idx)" &&

	# commits F, E, and D, should appear in that order at the
	# beginning of the pack
	test $f_pos -lt $e_pos &&
	test $e_pos -lt $d_pos &&

	# Ensure that the pack we are constructing sorts ahead of any
	# other packs in lexical/bitmap order by choosing it as the
	# preferred pack.
	git multi-pack-index write --bitmap --preferred-pack="pack-$D.idx" &&

	cat >in <<-EOF &&
	$(git rev-parse E)
	^$(git rev-parse D)
	EOF

	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --delta-base-offset --revs <in >/dev/null &&

	test_pack_reused 3 <trace2.txt &&
	test_packs_reused 1 <trace2.txt
'

test_expect_success 'reuse objects from middle pack with middle gap' '
	rm -fr $packdir/multi-pack-index* &&

	# Ensure that the pack we are constructing sort into any
	# position *but* the first one, by choosing a different pack as
	# the preferred one.
	git multi-pack-index write --bitmap --preferred-pack="pack-$A.idx" &&

	cat >in <<-EOF &&
	$(git rev-parse E)
	^$(git rev-parse D)
	EOF

	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --delta-base-offset --revs <in >/dev/null &&

	test_pack_reused 3 <trace2.txt &&
	test_packs_reused 1 <trace2.txt
'

test_expect_success 'omit delta with uninteresting base (same pack)' '
	git repack -adk &&

	test_seq 32 >f &&
	git add f &&
	test_tick &&
	git commit -m "delta" &&
	delta="$(git rev-parse HEAD)" &&

	test_seq 64 >f &&
	test_tick &&
	git commit -a -m "base" &&
	base="$(git rev-parse HEAD)" &&

	test_commit other &&

	git repack -d &&

	have_delta "$(git rev-parse $delta:f)" "$(git rev-parse $base:f)" &&

	git multi-pack-index write --bitmap &&

	cat >in <<-EOF &&
	$(git rev-parse other)
	^$base
	EOF

	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --delta-base-offset --revs <in >/dev/null &&

	# We can only reuse the 3 objects corresponding to "other" from
	# the latest pack.
	#
	# This is because even though we want "delta", we do not want
	# "base", meaning that we have to inflate the delta/base-pair
	# corresponding to the blob in commit "delta", which bypasses
	# the pack-reuse mechanism.
	#
	# The remaining objects from the other pack are similarly not
	# reused because their objects are on the uninteresting side of
	# the query.
	test_pack_reused 3 <trace2.txt &&
	test_packs_reused 1 <trace2.txt
'

test_expect_success 'omit delta from uninteresting base (cross pack)' '
	cat >in <<-EOF &&
	$(git rev-parse $base)
	^$(git rev-parse $delta)
	EOF

	P="$(git pack-objects --revs $packdir/pack <in)" &&

	git multi-pack-index write --bitmap --preferred-pack="pack-$P.idx" &&

	: >trace2.txt &&
	GIT_TRACE2_EVENT="$PWD/trace2.txt" \
		git pack-objects --stdout --delta-base-offset --all >/dev/null &&

	packs_nr="$(find $packdir -type f -name "pack-*.pack" | wc -l)" &&
	objects_nr="$(git rev-list --count --all --objects)" &&

	test_pack_reused $(($objects_nr - 1)) <trace2.txt &&
	test_packs_reused $packs_nr <trace2.txt
'

test_done
