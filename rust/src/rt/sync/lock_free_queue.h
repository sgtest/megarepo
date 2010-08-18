#ifndef LOCK_FREE_QUEUE_H
#define LOCK_FREE_QUEUE_H

#include "spin_lock.h"

class lock_free_queue_node {
public:
    lock_free_queue_node *next;
    lock_free_queue_node();
};

class lock_free_queue : lock_free_queue_node {
    spin_lock lock;
    lock_free_queue_node *_tail;
public:
    lock_free_queue();
    void enqueue(lock_free_queue_node *item);
    lock_free_queue_node *dequeue();
    bool is_empty();
};

#endif /* LOCK_FREE_QUEUE_H */
