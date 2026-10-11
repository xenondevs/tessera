package xyz.xenondevs.tessera.capture.dump

import com.mojang.blaze3d.vertex.PoseStack
import com.mojang.logging.LogUtils
import net.minecraft.client.Minecraft
import net.minecraft.client.renderer.item.ItemStackRenderState
import net.minecraft.resources.Identifier
import net.minecraft.world.item.Item
import net.minecraft.world.item.ItemDisplayContext
import net.minecraft.world.item.ItemStack
import xyz.xenondevs.tessera.capture.dump.sink.ItemRecord
import java.util.*

object ItemCapturer {
    
    private val LOGGER = LogUtils.getLogger()
    
    fun capture(id: Identifier, item: Item): ItemRecord? {
        val state = ItemStackRenderState()
        Minecraft.getInstance().itemModelResolver.updateForTopItem(state, ItemStack(item), ItemDisplayContext.GUI, null, null, 0)
        
        // TODO multiple special layers
        val (renderer, argument) = state.layers.firstNotNullOfOrNull { layer -> layer.specialRenderer?.let { it to layer.argumentForSpecialRendering } }
            ?: return null
        
        val collector = TesseraCollector()
        try {
            renderer.submit(argument, PoseStack(), collector, 0xF000F0, 0, false, 0)
        } catch (ex: Exception) {
            LOGGER.error("Error while capturing special item model of $id", ex)
            return ItemRecord(id, CaptureResult.Discarded(EnumSet.of(DiscardReason.EXCEPTION)))
        }
        return ItemRecord(id, collector.finish())
    }

//    fun capture(id: Identifier, item: Item): ItemRecord {
//        val leaves = ArrayList<CaptureResult>()
//        val record = ItemRecord(id, leaves)
//        val client = Minecraft.getInstance()
//        val resource = client.resourceManager.getResource(id.withPath { "items/$it.json" }).orElse(null)
//            ?: return record
//        val json = resource.openAsReader().use(JsonParser::parseReader)
//        val clientItem = ClientItem.CODEC.parse(JsonOps.INSTANCE, json).getOrThrow()
//
//        val unbakedLeaves = ArrayList<SpecialModelRenderer.Unbaked<*>>()
//        collect(clientItem.model, unbakedLeaves)
//        if (unbakedLeaves.isEmpty())
//            return record
//
//        val ctx = CaptureBakingContext(client)
//        val stack = ItemStack(item)
//
//        @Suppress("UNCHECKED_CAST")
//        unbakedLeaves.forEach { leaves += capture(it as SpecialModelRenderer.Unbaked<Any>, ctx, stack) }
//        return record
//    }

//    private fun collect(node: ItemModel.Unbaked, out: MutableList<SpecialModelRenderer.Unbaked<*>>) {
//        val pending = ArrayList<ItemModel.Unbaked>()
//        pending += node
//        while (pending.isNotEmpty()) {
//            when (val current = pending.removeFirst()) {
//                is SpecialModelWrapper.Unbaked -> out += current.specialModel
//                is CompositeModel.Unbaked -> current.models.asReversed().forEach { pending.addFirst(it) }
//                is SelectItemModel.Unbaked -> {
//                    current.fallback.ifPresent(pending::addFirst)
//                    current.unbakedSwitch.cases.asReversed().forEach { pending.addFirst(it.model) }
//                }
//
//                is ConditionalItemModel.Unbaked -> {
//                    pending.addFirst(current.onFalse)
//                    pending.addFirst(current.onTrue)
//                }
//
//                is RangeSelectItemModel.Unbaked -> {
//                    current.fallback.ifPresent(pending::addFirst)
//                    current.entries.asReversed().forEach { pending.addFirst(it.model) }
//                }
//            }
//        }
//    }
//
//    private fun capture(
//        unbaked: SpecialModelRenderer.Unbaked<Any>,
//        ctx: SpecialModelRenderer.BakingContext,
//        stack: ItemStack
//    ): CaptureResult {
//        val collector = TesseraCollector()
//        try {
//            val renderer = unbaked.bake(ctx)
//                ?: return CaptureResult.Discarded(EnumSet.of(DiscardReason.EXCEPTION))
//            renderer.submit(renderer.extractArgument(stack), PoseStack(), collector, 0xF000F0, 0, false, 0)
//        } catch (ex: Exception) {
//            LOGGER.error("Error while capturing special item model", ex)
//            return CaptureResult.Discarded(EnumSet.of(DiscardReason.EXCEPTION))
//        }
//        return collector.finish()
//    }

}

//private class CaptureBakingContext(private val client: Minecraft) : SpecialModelRenderer.BakingContext {
//    override fun entityModelSet() = client.entityModels
//
//    override fun sprites() = client.atlasManager
//
//    override fun playerSkinRenderCache() = client.playerSkinRenderCache()
//
//}
