// Throwaway checker for spikes/vdbread/ (P7's reading step, ADR-0045): OpenVDB's own
// library, through its public API, as a reader and writer apart from brodo's. Built by
// hand against OpenVDB 12.0.0, 13.0.0 and 14.0.0, never by brodo's build or gate.
//
//   oracle info           the library and file versions, and the compression a new file has
//   oracle default FILE   five grids written with that compression, nothing set: a float
//                         and a double level set, a double fog with tiles and a float grid
//                         whose leaves hold inactive values of every kind, and a double
//                         grid of 32768 active tiles in one upper node
//   oracle default FILE zip|none   the same grids with zip and active-mask, or with none
//   oracle dump FILE      every grid's name, type, class, background, transform, then each
//                         leaf's origin and its 512 values with their active bits, and each
//                         tile with its value and active bit, sorted
//   oracle small FILE default|zip|mask   brodo's test grids (`small` below)
//   oracle small FILE none   the grid of inactive values alone, uncompressed
//   oracle expect FILE    what brodo's reader is to give for each grid (ADR-0047)

#include <openvdb/openvdb.h>
#include <openvdb/io/File.h>
#include <openvdb/tools/SignedFloodFill.h>

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

namespace {

// The textbook |p - c| - r clamped to a band of 3 voxels, the band alone active, and the
// inside then flood-filled with -band, inactive, as a level set holds it.
template <typename GridT>
typename GridT::Ptr sphere(const std::string& name, typename GridT::ValueType r = 20,
    typename GridT::ValueType band = 3, bool positive = false)
{
    using V = typename GridT::ValueType;
    const V c[3] = {V(positive ? 13 : 3), V(positive ? 11 : -5), V(positive ? 12 : 7)};
    typename GridT::Ptr grid = GridT::create(band);
    auto acc = grid->getAccessor();
    for (int z = -24; z < 34; ++z) for (int y = -30; y < 20; ++y) for (int x = -22; x < 28; ++x) {
        const V dx = x - c[0], dy = (y - c[1]) * V(1.3), dz = (z - c[2]) * V(0.8);
        const V d = std::sqrt(dx * dx + dy * dy + dz * dz) - r;
        if (std::fabs(d) < band) acc.setValue(openvdb::Coord(x, y, z), d);
    }
    openvdb::tools::signedFloodFill(grid->tree());
    grid->setName(name);
    grid->setGridClass(openvdb::GRID_LEVEL_SET);
    return grid;
}

// A fog of 1 over a box of 160³ voxels, so that it holds active tiles at both internal
// levels, and a few distinct voxels beside it.
openvdb::DoubleGrid::Ptr fog()
{
    openvdb::DoubleGrid::Ptr grid = openvdb::DoubleGrid::create(0.0);
    grid->fill(openvdb::CoordBBox(openvdb::Coord(-16, 0, 0), openvdb::Coord(143, 159, 159)), 1.0, true);
    auto acc = grid->getAccessor();
    for (int k = 0; k < 40; ++k) acc.setValue(openvdb::Coord(300 + k, 2 * k, -k), 0.25 * k + 1e-12 * (k % 7));
    grid->setName("fog");
    grid->setGridClass(openvdb::GRID_FOG_VOLUME);
    openvdb::math::Transform::Ptr t = openvdb::math::Transform::createLinearTransform(0.5);
    t->postTranslate(openvdb::Vec3d(1.0, -2.0, 3.5));
    grid->setTransform(t);
    return grid;
}

// Four leaves whose inactive voxels hold one value not the background, the background and
// one other, two others, and three others, each with a few active voxels, so that every way
// of writing inactive values shows; on a non-uniform scale.
openvdb::FloatGrid::Ptr inactive()
{
    openvdb::FloatGrid::Ptr grid = openvdb::FloatGrid::create(2.0f);
    const float others[4][3] = {{7, 7, 7}, {2, 7, 2}, {7, 8, 7}, {7, 8, 9}};
    for (int leaf = 0; leaf < 4; ++leaf) {
        openvdb::FloatTree::LeafNodeType* node = grid->tree().touchLeaf(openvdb::Coord(8 * leaf, 0, 0));
        for (openvdb::Index i = 0; i < 512; ++i) node->setValueOff(i, others[leaf][i % 3]);
        for (openvdb::Index i = 0; i < 512; i += 37) node->setValueOn(i, float(i) * 0.5f - 3.0f);
    }
    grid->setName("inactive");
    grid->setTransform(openvdb::math::Transform::Ptr(new openvdb::math::Transform(
        openvdb::math::MapBase::Ptr(new openvdb::math::ScaleMap(openvdb::Vec3d(0.5, 0.25, 2.0))))));
    return grid;
}

// An upper node whose every place is an active tile of its own value, so that its values
// make a buffer of 32768 doubles, larger than one Blosc block.
openvdb::DoubleGrid::Ptr tiles()
{
    openvdb::DoubleGrid::Ptr grid = openvdb::DoubleGrid::create(-1.0);
    int k = 0;
    for (int x = 0; x < 32; ++x) for (int y = 0; y < 32; ++y) for (int z = 0; z < 32; ++z, ++k)
        grid->tree().addTile(2, openvdb::Coord(128 * x, 128 * y, 128 * z), 0.5 * k, true);
    grid->setName("tiles");
    return grid;
}

// brodo's test files (crates/brodo/tests/vdb_read.rs): the cases of `default` made small,
// each grid within one upper node, and the three kinds of grid brodo skips. A float and a
// double level set of radius 4 about (13, 11, 12); a
// double fog of 16³ voxels as eight active lower tiles, five distinct voxels and an active
// root tile, under a uniform scale and translation; the four leaves of inactive values, an
// inactive lower tile of another value, one of the background, an active upper tile and an
// inactive root tile of another value, under a non-uniform scale; a vector grid, a float
// grid saved as half floats, and an instance of the float level set.
openvdb::GridPtrVec small()
{
    openvdb::FloatGrid::Ptr sdf = sphere<openvdb::FloatGrid>("sdf", 4, 2, true);
    openvdb::DoubleGrid::Ptr dsdf = sphere<openvdb::DoubleGrid>("dsdf", 4, 2, true);
    openvdb::DoubleGrid::Ptr f = openvdb::DoubleGrid::create(0.0);
    f->fill(openvdb::CoordBBox(openvdb::Coord(0, 0, 0), openvdb::Coord(15, 15, 15)), 1.0, true);
    auto acc = f->getAccessor();
    for (int k = 0; k < 5; ++k) acc.setValue(openvdb::Coord(40 + k, 2 * k, -k), 0.25 * k + 1e-12 * (k % 7));
    f->tree().addTile(3, openvdb::Coord(4096, 0, 0), 0.5, true);
    f->setName("fog");
    f->setGridClass(openvdb::GRID_FOG_VOLUME);
    openvdb::math::Transform::Ptr t = openvdb::math::Transform::createLinearTransform(0.5);
    t->postTranslate(openvdb::Vec3d(1.0, -2.0, 3.5));
    f->setTransform(t);
    openvdb::FloatGrid::Ptr in = inactive();
    in->tree().addTile(1, openvdb::Coord(64, 0, 0), 5.0f, false);
    in->tree().addTile(1, openvdb::Coord(72, 0, 0), 2.0f, false);
    in->tree().addTile(2, openvdb::Coord(128, 0, 0), -4.0f, true);
    in->tree().addTile(3, openvdb::Coord(0, 4096, 0), 9.0f, false);
    openvdb::Vec3SGrid::Ptr vec = openvdb::Vec3SGrid::create();
    vec->tree().setValue(openvdb::Coord(1, 2, 3), openvdb::Vec3s(1, 2, 3));
    vec->setName("vec");
    openvdb::FloatGrid::Ptr half = openvdb::FloatGrid::create(0.0f);
    half->tree().setValue(openvdb::Coord(1, 2, 3), 1.5f);
    half->setName("half");
    half->setSaveFloatAsHalf(true);
    openvdb::GridBase::Ptr copy = sdf->copyGrid();
    copy->setName("sdf_instance");
    return openvdb::GridPtrVec{sdf, dsdf, f, in, vec, half, copy};
}

int writeSmall(const std::string& path, const std::string& compression)
{
    openvdb::io::File file(path);
    if (compression == "zip") file.setCompression(openvdb::io::COMPRESS_ZIP | openvdb::io::COMPRESS_ACTIVE_MASK);
    else if (compression == "mask") file.setCompression(openvdb::io::COMPRESS_ACTIVE_MASK);
    else if (compression == "none") {
        // Uncompressed, each upper node's values are written whole: the four leaves alone.
        file.setCompression(openvdb::io::COMPRESS_NONE);
        openvdb::FloatGrid::Ptr in = inactive();
        in->tree().addTile(1, openvdb::Coord(64, 0, 0), 5.0f, false);
        file.write(openvdb::GridPtrVec{in});
        file.close();
        return 0;
    }
    else if (compression != "default") { std::fprintf(stderr, "compression: default, zip, mask or none\n"); return 2; }
    file.write(small());
    file.close();
    return 0;
}

std::string hex(const void* p, size_t n)
{
    static const char* digits = "0123456789abcdef";
    std::string out;
    for (size_t i = 0; i < n; ++i) {
        const unsigned b = static_cast<const unsigned char*>(p)[i];
        out += digits[b >> 4];
        out += digits[b & 15];
    }
    return out;
}

// What brodo's reader is to give (ADR-0047), from OpenVDB's reading: each leaf's values in
// brodo's order, x fastest; each active tile, and each inactive one not holding the
// background, by bits; the transform as brodo's 3×4 map, its index v at v + ½ undone; the
// voxel size as the length of each of its columns.
template <typename GridT>
void expectTree(const GridT& grid)
{
    using V = typename GridT::ValueType;
    const V bg = grid.background();
    std::printf("background %s\n", hex(&bg, sizeof bg).c_str());
    std::vector<std::string> rows;
    for (auto it = grid.tree().cbeginLeaf(); it; ++it) {
        const openvdb::Coord o = it->origin();
        std::vector<V> values;
        for (int z = 0; z < 8; ++z) for (int y = 0; y < 8; ++y) for (int x = 0; x < 8; ++x)
            values.push_back(it->getValue(openvdb::Coord(o.x() + x, o.y() + y, o.z() + z)));
        char line[96];
        std::snprintf(line, sizeof line, "leaf %d %d %d ", o.x(), o.y(), o.z());
        rows.push_back(line + hex(values.data(), values.size() * sizeof(V)) + "\n");
    }
    for (auto it = grid.tree().cbeginValueAll(); it; ++it) {
        if (it.isVoxelValue()) continue;
        const V v = *it;
        if (!it.isValueOn() && std::memcmp(&v, &bg, sizeof v) == 0) continue;
        const openvdb::CoordBBox box = it.getBoundingBox();
        const int edge = box.dim().x();
        const char* level = edge == 4096 ? "root" : edge == 128 ? "upper" : "lower";
        char line[96];
        std::snprintf(line, sizeof line, "tile %s %d %d %d ", level, box.min().x(), box.min().y(), box.min().z());
        rows.push_back(line + hex(&v, sizeof v) + "\n");
    }
    std::sort(rows.begin(), rows.end());
    for (const auto& r : rows) std::fputs(r.c_str(), stdout);
}

int expect(const std::string& path)
{
    openvdb::io::File file(path);
    file.open();
    openvdb::GridPtrVecPtr grids = file.getGrids();
    file.close();
    for (const auto& base : *grids) {
        const bool half = base->saveFloatAsHalf();
        const bool kind = base->isType<openvdb::FloatGrid>() || base->isType<openvdb::DoubleGrid>()
            || base->isType<openvdb::Vec3SGrid>() || base->isType<openvdb::Vec3DGrid>();
        const bool shared = std::count_if(grids->begin(), grids->end(), [&](const openvdb::GridBase::Ptr& g) {
            return g->baseTreePtr() == base->baseTreePtr(); }) > 1 && base->getName() != "sdf";
        if (half || !kind || shared) { std::printf("skipped %s\n", base->getName().c_str()); continue; }
        std::printf("grid %s %s\n", base->getName().c_str(), base->type().c_str());
        for (auto m = base->beginMeta(); m != base->endMeta(); ++m) {
            const std::string key = m->first;
            if (m->second->typeName() != "string" || key == "name" || key.rfind("file_", 0) == 0) continue;
            std::printf("meta %s %s\n", key.c_str(), hex(m->second->str().data(), m->second->str().size()).c_str());
        }
        const openvdb::math::Mat4d M = base->transform().baseMap()->getAffineMap()->getMat4();
        double a[3][4];
        for (int r = 0; r < 3; ++r) {
            for (int c = 0; c < 3; ++c) a[r][c] = M(c, r);
            a[r][3] = M(3, r) - 0.5 * (a[r][0] + a[r][1] + a[r][2]);
        }
        double size[3];
        for (int c = 0; c < 3; ++c) size[c] = std::sqrt(a[0][c] * a[0][c] + a[1][c] * a[1][c] + a[2][c] * a[2][c]);
        std::printf("voxel_size %s\ntransform %s\n", hex(size, sizeof size).c_str(), hex(a, sizeof a).c_str());
        if (auto g = openvdb::gridPtrCast<openvdb::FloatGrid>(base)) expectTree(*g);
        else if (auto g = openvdb::gridPtrCast<openvdb::DoubleGrid>(base)) expectTree(*g);
        else if (auto g = openvdb::gridPtrCast<openvdb::Vec3SGrid>(base)) expectTree(*g);
        else if (auto g = openvdb::gridPtrCast<openvdb::Vec3DGrid>(base)) expectTree(*g);
    }
    return 0;
}

int info()
{
    openvdb::io::File file("unused.vdb");
    std::printf("library\t%s\tfile_version\t%u\tabi\t%d\n", OPENVDB_LIBRARY_VERSION_STRING,
        unsigned(openvdb::OPENVDB_FILE_VERSION), OPENVDB_ABI_VERSION_NUMBER);
    std::printf("default_compression\t%u\t%s\n", file.compression(),
        openvdb::io::compressionToString(file.compression()).c_str());
    return 0;
}

int writeDefault(const std::string& path, const std::string& compression)
{
    openvdb::GridPtrVec grids{sphere<openvdb::FloatGrid>("sdf"), sphere<openvdb::DoubleGrid>("dsdf"), fog(), inactive(), tiles()};
    openvdb::io::File file(path);
    if (compression == "zip") file.setCompression(openvdb::io::COMPRESS_ZIP | openvdb::io::COMPRESS_ACTIVE_MASK);
    else if (compression == "none") file.setCompression(openvdb::io::COMPRESS_NONE);
    else if (compression != "default") { std::fprintf(stderr, "compression: default, zip or none\n"); return 2; }
    file.write(grids);
    file.close();
    return 0;
}

template <typename GridT>
void dumpTree(const GridT& grid)
{
    std::vector<std::string> rows;
    char line[256];
    for (auto it = grid.tree().cbeginLeaf(); it; ++it) {
        const openvdb::Coord o = it->origin();
        std::string row;
        std::snprintf(line, sizeof line, "leaf\t%d\t%d\t%d", o.x(), o.y(), o.z());
        row += line;
        for (openvdb::Index i = 0; i < GridT::TreeType::LeafNodeType::SIZE; ++i) {
            std::snprintf(line, sizeof line, "\t%.17g:%d", static_cast<double>(it->getValue(i)), int(it->isValueOn(i)));
            row += line;
        }
        rows.emplace_back(row + "\n");
    }
    for (auto it = grid.tree().cbeginValueAll(); it; ++it) {
        if (it.isVoxelValue()) continue;
        const openvdb::CoordBBox box = it.getBoundingBox();
        std::snprintf(line, sizeof line, "tile\t%d\t%d\t%d\t%d\t%.17g:%d\n", box.min().x(), box.min().y(),
            box.min().z(), box.dim().x(), static_cast<double>(*it), int(it.isValueOn()));
        rows.emplace_back(line);
    }
    std::sort(rows.begin(), rows.end());
    for (const auto& r : rows) std::fputs(r.c_str(), stdout);
}

int dump(const std::string& path)
{
    openvdb::io::File file(path);
    file.open();
    std::printf("file\tversion\t%u\n", file.fileVersion());
    for (auto name = file.beginName(); name != file.endName(); ++name) {
        openvdb::GridBase::Ptr base = file.readGrid(name.gridName());
        std::printf("grid\t%s\ttype\t%s\tclass\t%s\tvoxels\t%llu\n", base->getName().c_str(),
            base->type().c_str(), openvdb::GridBase::gridClassToString(base->getGridClass()).c_str(),
            static_cast<unsigned long long>(base->activeVoxelCount()));
        const openvdb::math::Mat4d m = base->transform().baseMap()->getAffineMap()->getMat4();
        std::printf("map\t%s", base->transform().mapType().c_str());
        for (int r = 0; r < 4; ++r) for (int c = 0; c < 4; ++c) std::printf("\t%.17g", m(r, c));
        std::printf("\n");
        if (auto g = openvdb::gridPtrCast<openvdb::FloatGrid>(base)) {
            std::printf("background\t%.17g\n", static_cast<double>(g->background()));
            dumpTree(*g);
        } else if (auto g = openvdb::gridPtrCast<openvdb::DoubleGrid>(base)) {
            std::printf("background\t%.17g\n", g->background());
            dumpTree(*g);
        } else {
            std::printf("unhandled\n");
        }
    }
    file.close();
    return 0;
}

} // namespace

int main(int argc, char** argv)
{
    openvdb::initialize();
    const std::string mode = argc > 1 ? argv[1] : "";
    try {
        if (mode == "info" && argc == 2) return info();
        if (mode == "default" && argc == 3) return writeDefault(argv[2], "default");
        if (mode == "default" && argc == 4) return writeDefault(argv[2], argv[3]);
        if (mode == "dump" && argc == 3) return dump(argv[2]);
        if (mode == "small" && argc == 4) return writeSmall(argv[2], argv[3]);
        if (mode == "expect" && argc == 3) return expect(argv[2]);
    } catch (const std::exception& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
    std::fprintf(stderr, "usage: oracle info | oracle default FILE [zip|none] | oracle dump FILE | oracle small FILE default|zip|mask|none | oracle expect FILE\n");
    return 2;
}
