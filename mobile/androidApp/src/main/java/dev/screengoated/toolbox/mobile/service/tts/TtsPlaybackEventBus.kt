package dev.screengoated.toolbox.mobile.service.tts

import java.util.concurrent.LinkedBlockingDeque
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch

/** Structural interest in terminal events. A missing prefix selects all owners. */
data class TtsEventSelector(val consumer: TtsConsumer, val ownerPrefix: String? = null) {
    init { require(ownerPrefix == null || ownerPrefix.isNotEmpty()) }
    internal fun matches(consumer: TtsConsumer, owner: String): Boolean =
        this.consumer == consumer && (ownerPrefix == null || owner.startsWith(ownerPrefix))
}

/**
 * Reserves bounded terminal-event capacity before a request can have effects.
 * Every subscriber has its own bounded mailbox; acknowledgment is after its
 * handler returns, so a slow subscriber does not drop events for other owners.
 */
internal class TtsPlaybackEventBus(
    private val maxOutstanding: Int = 512,
    private val maxPerOwner: Int = 32,
    private val maxPerSubscriber: Int = 32,
    private val maxPerConsumer: Int = (maxOutstanding / TtsConsumer.entries.size).coerceAtLeast(1),
) {
    init { require(maxOutstanding > 0 && maxPerOwner > 0 && maxPerSubscriber > 0 && maxPerConsumer > 0) }
    private data class Owner(val consumer: TtsConsumer, val token: String)
    private data class Reservation(
        val owner: Owner,
        val claims: MutableSet<Long>,
        var waiting: MutableSet<Long>? = null,
    )
    private data class Delivery(val event: TtsPlaybackEvent, val requestId: Long)
    private data class Subscriber(
        val selectors: List<TtsEventSelector>,
        val mailbox: Channel<Delivery>,
        var claims: Int = 0,
    )

    private val lock = Any()
    private val requests = LinkedHashMap<Long, Reservation>()
    private val owners = HashMap<Owner, Int>()
    private val consumers = HashMap<TtsConsumer, Int>()
    private val subscribers = LinkedHashMap<Long, Subscriber>()
    private var nextSubscriber = 1L
    private val deliveryScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    fun reserve(requestId: Long, consumer: TtsConsumer, ownerToken: String): Boolean = synchronized(lock) {
        val owner = Owner(consumer, ownerToken)
        val matching = subscribers.filterValues { sub -> sub.selectors.any { it.matches(consumer, ownerToken) } }
        if (requestId <= 0 || requests.containsKey(requestId) || requests.size >= maxOutstanding ||
            (owners[owner] ?: 0) >= maxPerOwner || (consumers[consumer] ?: 0) >= maxPerConsumer ||
            matching.values.any { it.claims >= maxPerSubscriber }) return@synchronized false
        requests[requestId] = Reservation(owner, matching.keys.toMutableSet())
        matching.values.forEach { it.claims++ }
        owners[owner] = (owners[owner] ?: 0) + 1
        consumers[consumer] = (consumers[consumer] ?: 0) + 1
        true
    }

    /** Only for a request rejected before it was placed into either work queue. */
    fun abandon(requestId: Long) = synchronized(lock) { release(requestId) }

    fun publish(event: TtsPlaybackEvent) = synchronized(lock) {
        val reservation = requests[event.requestId] ?: return@synchronized
        if (reservation.waiting != null || reservation.owner != Owner(event.consumer, event.ownerToken)) return@synchronized
        val targets = subscribers.entries.filter { (_, subscriber) ->
            subscriber.selectors.any { it.matches(event.consumer, event.ownerToken) }
        }.map { it.key to it.value }
        val waiting = targets.mapTo(linkedSetOf()) { it.first }
        reservation.waiting = waiting
        for ((id, subscriber) in targets) {
            // There can be at most maxOutstanding unacknowledged reservations,
            // so a live subscriber mailbox of that capacity cannot overflow.
            if (!subscriber.mailbox.trySend(Delivery(event, event.requestId)).isSuccess) waiting.remove(id)
        }
        if (waiting.isEmpty()) release(event.requestId)
    }

    fun eventsFor(vararg selectors: TtsEventSelector): Flow<TtsPlaybackEvent> {
        require(selectors.isNotEmpty())
        val interests = selectors.toList()
        return flow {
            val mailbox = Channel<Delivery>(maxOutstanding)
            val output = Channel<Delivery>()
            val forwarder = deliveryScope.launch {
                for (delivery in mailbox) output.send(delivery)
            }
            val subscriberId = synchronized(lock) {
                val id = nextSubscriber++
                val subscriber = Subscriber(interests, mailbox)
                subscribers[id] = subscriber
                for (reservation in requests.values) {
                    if (reservation.waiting == null && interests.any {
                            it.matches(reservation.owner.consumer, reservation.owner.token)
                        }) {
                        reservation.claims.add(id)
                        subscriber.claims++
                    }
                }
                id
            }
            try {
                for (delivery in output) {
                    emit(delivery.event)
                    acknowledge(subscriberId, delivery.requestId)
                }
            } finally {
                unsubscribe(subscriberId)
                mailbox.cancel()
                forwarder.cancel()
                output.cancel()
            }
        }
    }

    private fun acknowledge(subscriberId: Long, requestId: Long) = synchronized(lock) {
        val waiting = requests[requestId]?.waiting ?: return@synchronized
        waiting.remove(subscriberId)
        val reservation = requests[requestId] ?: return@synchronized
        if (reservation.claims.remove(subscriberId)) subscribers[subscriberId]?.let { it.claims-- }
        if (waiting.isEmpty()) release(requestId)
    }

    private fun unsubscribe(subscriberId: Long) = synchronized(lock) {
        subscribers.remove(subscriberId) ?: return@synchronized
        val complete = mutableListOf<Long>()
        for ((requestId, reservation) in requests) {
            reservation.claims.remove(subscriberId)
            val waiting = reservation.waiting ?: continue
            waiting.remove(subscriberId)
            if (waiting.isEmpty()) complete.add(requestId)
        }
        complete.forEach(::release)
    }

    /** Caller owns lock. */
    private fun release(requestId: Long) {
        val reservation = requests.remove(requestId) ?: return
        val owner = reservation.owner
        reservation.claims.forEach { id -> subscribers[id]?.let { it.claims-- } }
        val count = (owners[owner] ?: 1) - 1
        if (count <= 0) owners.remove(owner) else owners[owner] = count
        val consumerCount = (consumers[owner.consumer] ?: 1) - 1
        if (consumerCount <= 0) consumers.remove(owner.consumer) else consumers[owner.consumer] = consumerCount
    }
}

/** Queue ownership transfers to either the player or interruption, never both. */
internal fun <T> retireQueuedPlayback(queue: LinkedBlockingDeque<T>, retire: (T) -> Unit) {
    val removed = ArrayList<T>()
    queue.drainTo(removed)
    removed.forEach(retire)
}

/** Transfer only the matching queued request; a concurrent dequeue owns its own terminal. */
internal fun <T> retireMatchingQueuedPlayback(
    queue: LinkedBlockingDeque<T>,
    matches: (T) -> Boolean,
    retire: (T) -> Unit,
): Boolean {
    val item = queue.firstOrNull(matches) ?: return false
    if (!queue.remove(item)) return false
    retire(item)
    return true
}

internal fun matchesRealtimeOwner(
    requestId: Long,
    ownerToken: String,
    candidateId: Long,
    candidateConsumer: TtsConsumer,
    candidateOwner: String,
): Boolean = candidateId == requestId && candidateConsumer == TtsConsumer.REALTIME && candidateOwner == ownerToken
