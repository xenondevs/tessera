package xyz.xenondevs.tessera.capture.dump

import net.minecraft.client.ClientClockManager
import net.minecraft.client.multiplayer.ClientLevel
import net.minecraft.core.BlockPos
import net.minecraft.core.Holder
import net.minecraft.core.RegistryAccess
import net.minecraft.core.particles.ExplosionParticleInfo
import net.minecraft.core.particles.ParticleOptions
import net.minecraft.core.registries.Registries
import net.minecraft.sounds.SoundEvent
import net.minecraft.sounds.SoundSource
import net.minecraft.util.random.WeightedList
import net.minecraft.world.Difficulty
import net.minecraft.world.TickRateManager
import net.minecraft.world.attribute.EnvironmentAttributeSystem
import net.minecraft.world.damagesource.DamageSource
import net.minecraft.world.entity.Entity
import net.minecraft.world.entity.boss.enderdragon.EnderDragonPart
import net.minecraft.world.entity.player.Player
import net.minecraft.world.flag.FeatureFlagSet
import net.minecraft.world.item.crafting.RecipeAccess
import net.minecraft.world.level.ExplosionDamageCalculator
import net.minecraft.world.level.Level
import net.minecraft.world.level.LightLayer
import net.minecraft.world.level.biome.Biome
import net.minecraft.world.level.block.Block
import net.minecraft.world.level.block.Blocks
import net.minecraft.world.level.block.LevelEvent
import net.minecraft.world.level.block.state.BlockState
import net.minecraft.world.level.border.WorldBorder
import net.minecraft.world.level.chunk.ChunkAccess
import net.minecraft.world.level.chunk.ChunkSource
import net.minecraft.world.level.chunk.status.ChunkStatus
import net.minecraft.world.level.dimension.BuiltinDimensionTypes
import net.minecraft.world.level.entity.LevelEntityGetter
import net.minecraft.world.level.gameevent.GameEvent
import net.minecraft.world.level.lighting.LevelLightEngine
import net.minecraft.world.level.material.Fluid
import net.minecraft.world.level.saveddata.maps.MapId
import net.minecraft.world.level.storage.LevelData
import net.minecraft.world.phys.Vec3
import net.minecraft.world.scores.Scoreboard
import net.minecraft.world.ticks.BlackholeTickAccess
import xyz.xenondevs.tessera.capture.util.WorldArea
import java.util.function.BooleanSupplier

class CaptureLevel(
    access: RegistryAccess,
    val enabledFeatures: FeatureFlagSet,
    area: WorldArea
) : Level(
    ClientLevel.ClientLevelData(Difficulty.NORMAL, false, false),
    OVERWORLD,
    access,
    access.lookupOrThrow(Registries.DIMENSION_TYPE).getOrThrow(BuiltinDimensionTypes.OVERWORLD),
    true,
    false,
    0L,
    0
) {
    
    var area: WorldArea = area
        set(value) {
            field = value
            area.blockEntities.values.forEach { it.setLevel(this) }
        }
    
    init {
        this.area = area
    }
    
    private val tickRateManager = TickRateManager()
    private val scoreboard = Scoreboard()
    private val clockManager = ClientClockManager()
    private val worldBorder = WorldBorder()
    private val chunkSource = EmptyChunkSource()
    
    override fun getBlockState(pos: BlockPos) =
        area.getBlockState(pos) ?: Blocks.VOID_AIR.defaultBlockState()
    
    override fun getFluidState(pos: BlockPos) =
        getBlockState(pos).fluidState
    
    override fun getBlockEntity(pos: BlockPos) =
        area.getBlockEntity(pos)
    
    override fun getGameTime() = 0L
    
    override fun getBrightness(layer: LightLayer, pos: BlockPos) = 15
    
    override fun getLightEngine() = LevelLightEngine.EMPTY
    
    override fun getEntity(id: Int): Entity? = null
    
    override fun sendBlockUpdated(pos: BlockPos, old: BlockState, current: BlockState, updateFlags: @Block.UpdateFlags Int) = Unit
    
    override fun playSeededSound(except: Entity?, x: Double, y: Double, z: Double, sound: Holder<SoundEvent>, source: SoundSource, volume: Float, pitch: Float, seed: Long) = Unit
    
    override fun playSeededSound(except: Entity?, sourceEntity: Entity, sound: Holder<SoundEvent>, source: SoundSource, volume: Float, pitch: Float, seed: Long) = Unit
    
    override fun explode(source: Entity?, damageSource: DamageSource?, damageCalculator: ExplosionDamageCalculator?, x: Double, y: Double, z: Double, r: Float, fire: Boolean, interactionType: ExplosionInteraction, smallExplosionParticles: ParticleOptions, largeExplosionParticles: ParticleOptions, blockParticles: WeightedList<ExplosionParticleInfo>, explosionSound: Holder<SoundEvent>) = Unit
    
    override fun gatherChunkSourceStats() = ""
    
    override fun setRespawnData(respawnData: LevelData.RespawnData) = Unit
    
    override fun getRespawnData() = LevelData.RespawnData.DEFAULT
    
    override fun dragonParts(): Collection<EnderDragonPart> = emptyList()
    
    override fun tickRateManager() = tickRateManager
    
    override fun getMapData(id: MapId) = null
    
    override fun destroyBlockProgress(id: Int, blockPos: BlockPos, progress: Int) = Unit
    
    override fun getScoreboard() = scoreboard
    
    override fun getChunkSource(): ChunkSource = chunkSource
    
    override fun blockEntityChanged(pos: BlockPos) = Unit
    
    override fun recipeAccess(): RecipeAccess {
        TODO("Not yet implemented")
    }
    
    override fun getEntities(): LevelEntityGetter<Entity> {
        TODO("Not yet implemented")
    }
    
    override fun clockManager() = clockManager
    
    override fun environmentAttributes(): EnvironmentAttributeSystem {
        TODO("Not yet implemented")
    }
    
    override fun levelEvent(source: Entity?, type: @LevelEvent.Value Int, pos: BlockPos, data: Int) = Unit
    
    override fun gameEvent(gameEvent: Holder<GameEvent>, position: Vec3, context: GameEvent.Context) = Unit
    
    override fun getUncachedNoiseBiome(quartX: Int, quartY: Int, quartZ: Int): Holder<Biome> {
        TODO("Not yet implemented")
    }
    
    override fun getSeaLevel() = 0
    
    override fun enabledFeatures() = enabledFeatures
    
    override fun getWorldBorder() = worldBorder
    
    override fun players() = emptyList<Player>()
    
    override fun getBlockTicks() = BlackholeTickAccess.emptyLevelList<Block>()
    
    override fun getFluidTicks() = BlackholeTickAccess.emptyLevelList<Fluid>()
    
    // TODO, report chunks inside the areas as present and loaded
    private inner class EmptyChunkSource : ChunkSource() {
        override fun getChunk(x: Int, z: Int, targetStatus: ChunkStatus, loadOrGenerate: Boolean): ChunkAccess? = null
        
        override fun tick(haveTime: BooleanSupplier, tickChunks: Boolean) = Unit
        
        override fun gatherStats() = "capture level chunk source"
        
        override fun getLoadedChunksCount() = 0
        
        override fun getLightEngine() = LevelLightEngine.EMPTY
        
        override fun getLevel() = this@CaptureLevel
        
    }
    
}

