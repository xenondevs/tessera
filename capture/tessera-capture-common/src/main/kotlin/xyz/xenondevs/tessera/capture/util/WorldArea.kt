package xyz.xenondevs.tessera.capture.util

import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap
import net.minecraft.core.BlockPos
import net.minecraft.world.level.block.entity.BlockEntity
import net.minecraft.world.level.block.state.BlockState

class WorldArea(
    val bounds: Area,
    val blocks: Array<BlockState?>,
    val blockEntities: Int2ObjectOpenHashMap<BlockEntity>
) {
    
    fun getIndex(pos: BlockPos): Int {
        if (pos !in bounds)
            return -1
        
        val x = pos.x - bounds.min.x
        val y = pos.y - bounds.min.y
        val z = pos.z - bounds.min.z
        val width = bounds.width
        val height = bounds.height
        
        return x + y * width + z * width * height
    }
    
    fun getBlockState(pos: BlockPos): BlockState? {
        val index = getIndex(pos)
        if (index == -1)
            return null
        
        return blocks[index]
    }
    
    fun getBlockEntity(pos: BlockPos): BlockEntity? {
        val index = getIndex(pos)
        if (index == -1)
            return null
        
        return blockEntities[index]
    }
    
    companion object {
        
        @JvmStatic
        val EMPTY = WorldArea(Area(BlockPos.ZERO, BlockPos.ZERO), emptyArray(), Int2ObjectOpenHashMap())
        
        fun newSingleBlockArea(state: BlockState, blockEntity: BlockEntity? = null): WorldArea {
            val blocks: Array<BlockState?> = arrayOf(state)
            
            val blockEntities = Int2ObjectOpenHashMap<BlockEntity>()
            if (blockEntity != null) {
                blockEntities[0] = blockEntity
            }
            
            return WorldArea(Area(BlockPos.ZERO, BlockPos(1, 1, 1)), blocks, blockEntities)
        }
        
    }
    
}

data class Area(
    val min: BlockPos,
    val maxExclusive: BlockPos
) {
    
    val width = maxExclusive.x - min.x
    val height = maxExclusive.y - min.y
    
    operator fun contains(other: BlockPos): Boolean {
        return other.x in min.x until maxExclusive.x &&
            other.y in min.y until maxExclusive.y &&
            other.z in min.z until maxExclusive.z
    }
    
    companion object {
        fun of(first: BlockPos, second: BlockPos): Area {
            val min = BlockPos(
                minOf(first.x, second.x),
                minOf(first.y, second.y),
                minOf(first.z, second.z)
            )
            val max = BlockPos(
                maxOf(first.x, second.x) + 1,
                maxOf(first.y, second.y) + 1,
                maxOf(first.z, second.z) + 1
            )
            return Area(min, max)
        }
    }
    
}
