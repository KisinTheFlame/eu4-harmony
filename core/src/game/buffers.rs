//! The game's memory as a test lays it out: an object of the game's in a
//! buffer of the test's, its fields set at the offsets `crate::objects`
//! names ([`Object`]), and the game's image, its globals and functions put at
//! their RVAs ([`Image`]). A capability then runs through its interface over
//! a [`Session`] made of them ([`session`]), reading and calling them as it
//! reads and calls the game's: through [`Memory`], under the same guard.
use super::{commands::Commands, Caller, Ctx, Session, Tag, View};
use crate::{
    game_string,
    memory::Memory,
    objects::{campaign::country, list, map},
};
use windows_sys::Win32::System::{
    Diagnostics::Debug::FlushInstructionCache,
    Memory::{VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_NOACCESS},
    Threading::GetCurrentProcess,
};

/// An object of the game's, in a buffer of the test's: zero until a field is
/// set, and the objects it points to kept with it. The game's code reads and
/// writes it through its address, so it is written through that too.
pub struct Object {
    at: *mut u64,
    words: usize,
    kept: Vec<Object>,
    vectors: Vec<Box<[u64]>>,
}

impl Object {
    /// `size` bytes, all zero.
    pub fn new(size: usize) -> Self {
        let words = size.div_ceil(8).max(1);
        Self { at: Box::into_raw(vec![0u64; words].into_boxed_slice()) as *mut u64, words, kept: Vec::new(), vectors: Vec::new() }
    }

    /// A country of the game's with `tag`, `size` bytes or as far as its tag.
    pub fn country(tag: Tag, size: usize) -> Self {
        Self::new(size.max(country::TAG + 8)).u64(country::TAG, tag.0)
    }

    /// The game's list of countries ([`Ctx::countries`]), with room for
    /// every index a tag has: each of `countries` at its tag's index, read
    /// from the tag it holds, and none at the others.
    pub fn countries(countries: &[&Object]) -> Self {
        let tags: Vec<Tag> = countries.iter().map(|c| Tag(c.read_u64(country::TAG))).collect();
        let mut list = Self::new((i16::MAX as usize + 1) * 8);
        for (tag, object) in tags.iter().zip(countries) {
            list = list.u64(tag.index() as usize * 8, object.at() as u64);
        }
        list
    }

    /// Where it is.
    pub fn at(&self) -> usize {
        self.at as usize
    }

    /// Writes `bytes` at `offset`, as the game's code writes it.
    pub fn set(&self, offset: usize, bytes: &[u8]) {
        assert!(offset + bytes.len() <= self.words * 8, "{offset:#x}+{} is past the object's {:#x} bytes", bytes.len(), self.words * 8);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), (self.at as *mut u8).add(offset), bytes.len()) };
    }

    fn read<const N: usize>(&self, offset: usize) -> [u8; N] {
        assert!(offset + N <= self.words * 8, "{offset:#x}+{N} is past the object's {:#x} bytes", self.words * 8);
        let mut out = [0; N];
        unsafe { std::ptr::copy_nonoverlapping((self.at as *const u8).add(offset), out.as_mut_ptr(), N) };
        out
    }

    pub fn read_u64(&self, offset: usize) -> u64 {
        u64::from_le_bytes(self.read(offset))
    }

    pub fn read_i32(&self, offset: usize) -> i32 {
        i32::from_le_bytes(self.read(offset))
    }

    pub fn u8(self, offset: usize, value: u8) -> Self {
        self.set(offset, &[value]);
        self
    }

    pub fn i32(self, offset: usize, value: i32) -> Self {
        self.set(offset, &value.to_le_bytes());
        self
    }

    pub fn u32(self, offset: usize, value: u32) -> Self {
        self.set(offset, &value.to_le_bytes());
        self
    }

    pub fn u64(self, offset: usize, value: u64) -> Self {
        self.set(offset, &value.to_le_bytes());
        self
    }

    /// A pointer at `offset` to `object`, which it keeps.
    pub fn points_to(mut self, offset: usize, object: Object) -> Self {
        self = self.u64(offset, object.at() as u64);
        self.kept.push(object);
        self
    }

    /// One of the game's lists at `offset` (`objects::list`), holding
    /// `items` in order, each with whether the game removed its node while
    /// walking the list; the nodes and the items are kept.
    pub fn list(mut self, offset: usize, items: Vec<(Object, bool)>) -> Self {
        let count = items.len();
        let nodes: Vec<Object> =
            items.into_iter().map(|(item, removed)| Object::new(list::NODE_SIZE).u8(list::REMOVED, removed.into()).points_to(list::ITEM, item)).collect();
        for i in 0..count {
            let previous = if i == 0 { 0 } else { nodes[i - 1].at() as u64 };
            let next = nodes.get(i + 1).map_or(0, |n| n.at() as u64);
            nodes[i].set(list::PREVIOUS, &previous.to_le_bytes());
            nodes[i].set(list::NEXT, &next.to_le_bytes());
        }
        let (head, tail) = (nodes.first().map_or(0, |n| n.at() as u64), nodes.last().map_or(0, |n| n.at() as u64));
        self = self.u64(offset + list::HEAD, head).u64(offset + list::TAIL, tail).i32(offset + list::COUNT, count as i32);
        self.kept.extend(nodes);
        self
    }

    /// The head of a `std::map` (`objects::map`) holding `values`, each the
    /// first 8 bytes of a node's value: a root and a chain of right
    /// children, every other child the head, as the game's empty ones are.
    pub fn map(values: &[u64]) -> Self {
        let head = Self::new(map::NODE_SIZE).u8(map::NIL, 1);
        let mut nodes: Vec<Object> = values.iter().map(|&v| Self::new(map::NODE_SIZE).u64(map::VALUE, v)).collect();
        let at = |n: &Object| n.at() as u64;
        for i in 0..nodes.len() {
            let parent = if i == 0 { at(&head) } else { at(&nodes[i - 1]) };
            let right = nodes.get(i + 1).map_or(at(&head), at);
            nodes[i].set(map::PARENT, &parent.to_le_bytes());
            nodes[i].set(map::LEFT, &at(&head).to_le_bytes());
            nodes[i].set(map::RIGHT, &right.to_le_bytes());
        }
        let root = nodes.first().map_or(at(&head), at);
        let mut head = head.u64(map::PARENT, root);
        head.kept.append(&mut nodes);
        head
    }

    /// A `std::vector` of `items` at `offset`: its begin and end.
    pub fn vector(mut self, offset: usize, items: &[u64]) -> Self {
        let items: Box<[u64]> = items.into();
        let begin = items.as_ptr() as u64;
        self = self.u64(offset, begin).u64(offset + 8, begin + 8 * items.len() as u64);
        self.vectors.push(items);
        self
    }

    /// A `std::string` at `offset` holding `text` in itself, as one of at
    /// most 15 bytes is held.
    pub fn string(self, offset: usize, text: &str) -> Self {
        assert!(text.len() <= game_string::INLINE, "{text:?} is longer than a string holds in itself");
        self.set(offset, text.as_bytes());
        self.u64(offset + game_string::SIZE, text.len() as u64).u64(offset + game_string::CAPACITY, game_string::INLINE as u64)
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.at, self.words)) });
    }
}

/// Past the furthest RVA a dependency point names.
const IMAGE: usize = 40 << 20;
const PAGE: usize = 0x1000;

/// The game's image, for a test: reserved as far as the game's, with only
/// what the test puts there, the game's globals and its functions at their
/// RVAs. A function is the test's own, reached from its RVA by a jump.
pub struct Image {
    base: usize,
}

impl Image {
    pub fn new() -> Self {
        let base = unsafe { VirtualAlloc(std::ptr::null(), IMAGE, MEM_RESERVE, PAGE_NOACCESS) } as usize;
        assert!(base != 0, "reserving a test's image failed");
        Self { base }
    }

    /// The game's memory with this image, as capabilities read it.
    pub fn memory(&self) -> &'static Memory {
        Box::leak(Box::new(Memory { base: self.base }))
    }

    /// The bytes at `rva`, committed first.
    fn put(&self, rva: usize, bytes: &[u8]) {
        assert!(rva + bytes.len() <= IMAGE, "{rva:#x} is past the test's image");
        let first = rva / PAGE * PAGE;
        let committed = unsafe { VirtualAlloc((self.base + first) as _, rva + bytes.len() - first, MEM_COMMIT, PAGE_EXECUTE_READWRITE) };
        assert!(!committed.is_null(), "committing the test's image at {rva:#x} failed");
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), (self.base + rva) as *mut u8, bytes.len()) };
    }

    /// A global of the game's at `rva`.
    pub fn i32(&self, rva: usize, value: i32) -> &Self {
        self.put(rva, &value.to_le_bytes());
        self
    }

    pub fn u64(&self, rva: usize, value: u64) -> &Self {
        self.put(rva, &value.to_le_bytes());
        self
    }

    /// The game's function at `rva`, as `function`: `mov rax, function;
    /// jmp rax`, which leaves its arguments as the caller passed them.
    pub fn function(&self, rva: usize, function: *const ()) -> &Self {
        let mut jump = vec![0x48, 0xb8];
        jump.extend_from_slice(&(function as u64).to_le_bytes());
        jump.extend_from_slice(&[0xff, 0xe0]);
        self.put(rva, &jump);
        unsafe { FlushInstructionCache(GetCurrentProcess(), (self.base + rva) as _, jump.len()) };
        self
    }
}

impl Default for Image {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe { VirtualFree(self.base as _, 0, MEM_RELEASE) };
    }
}

/// The session of the player `player`, whose country is the object at
/// `country`, among the countries' list at `countries`, in the world at
/// `world`, as `caller` reads it in the full view; the commands it posts are
/// recorded (`commands::record`).
pub fn session(m: &'static Memory, world: usize, player: Tag, country: usize, countries: usize, caller: Caller) -> Session {
    Session { m, ctx: Ctx { world, date: 0, player, country, countries }, view: View::Full, caller, commands: Commands::Recorded }
}

// ---------- the units, diplomacy, peace, time, console, subjects and
// trade companies' layer tests ----------

/// The world session of the player `player` in the world at `world` on
/// `date`, among the countries' list at `countries`, as `caller` reads it in
/// the full view; the commands it posts are recorded.
pub fn world_session(m: &'static Memory, world: usize, date: i32, player: Tag, countries: usize, caller: Caller) -> super::WorldSession {
    let world = super::World { world, date, countries, player: super::player::Player::Plays(player) };
    super::WorldSession { m, world, view: View::Full, caller, commands: Commands::Recorded }
}

// The provinces, envoys, units and the game's answers, for the action
// capabilities of leaders, mercenaries, merchants, missionaries, missions,
// parliament, questions, recruiting, sieges, offers and occupation: the
// world's provinces as the map and the world hold them, each read through
// the geography the capabilities keep of them.

/// Held while a test's provinces stand in the capabilities' geography, the
/// process's one, so that tests running at once do not read each other's.
static GEOGRAPHY: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Whether this thread's test holds [`GEOGRAPHY`].
    static HOLDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// How far a test's world reaches.
const WORLD: usize = 0x4000;

/// The world's provinces, numbered from 0, the placeholder, to `count - 1`,
/// each on land with a geometry of its own and no neighbors; the map at its
/// global in `image`, and the world pointing to the same provinces, its
/// other fields as far as [`WORLD`] set with [`Provinces::world_set`].
pub struct Provinces {
    array: Object,
    world: Object,
    map: Object,
    _kept: Vec<Object>,
    _held: std::sync::MutexGuard<'static, ()>,
}

impl Provinces {
    pub fn new(image: &Image, count: usize) -> Self {
        use crate::objects::campaign::{geometry, globals, map, province, world};
        // A second world on the thread that holds one would wait for itself.
        assert!(!HOLDING.with(|h| h.replace(true)), "a test has one world of provinces at a time");
        let held = GEOGRAPHY.lock().unwrap_or_else(|e| e.into_inner());
        super::with_state(|s| s.geography.clear());
        let array = Object::new(count * province::SIZE);
        let mut kept = Vec::new();
        for id in 0..count {
            let shape = Object::new(geometry::CLIMATE + 8).u32(geometry::ID, id as u32).u8(geometry::TERRAIN, geometry::LAND);
            array.set(id * province::SIZE + province::ID, &(id as u32).to_le_bytes());
            array.set(id * province::SIZE + province::GEOMETRY, &(shape.at() as u64).to_le_bytes());
            kept.push(shape);
        }
        let first = array.at() as u64;
        let end = first + (count * province::SIZE) as u64;
        let the_map = Object::new(map::PROVINCES + 0x10).u64(map::PROVINCES, first).u64(map::PROVINCES + 8, end);
        image.u64(globals::MAP, the_map.at() as u64);
        let world = Object::new(WORLD.max(world::PROVINCES.max(world::MAP_MANAGER) + 8)).u64(world::PROVINCES, first);
        Self { array, world, map: the_map, _kept: kept, _held: held }
    }

    /// The world, for [`session`].
    pub fn world(&self) -> usize {
        self.world.at()
    }

    /// Writes `bytes` at `offset` of the world.
    pub fn world_set(&self, offset: usize, bytes: &[u8]) -> &Self {
        self.world.set(offset, bytes);
        self
    }

    /// The world's object, for a test to set its other fields.
    pub fn world_object(&self) -> &Object {
        &self.world
    }

    /// The map's object, the game's global `MAP`, for a test to set its
    /// other fields.
    pub fn map(&self) -> &Object {
        &self.map
    }

    /// Where province `id` is.
    pub fn province(&self, id: u32) -> usize {
        self.at(id)
    }

    /// The province `id` owned by `owner`.
    pub fn owner(&self, id: u32, owner: Tag) -> &Self {
        self.u64(id, crate::objects::campaign::province::OWNER, owner.0)
    }

    /// Where province `id` is.
    pub fn at(&self, id: u32) -> usize {
        self.array.at() + id as usize * crate::objects::campaign::province::SIZE
    }

    /// Writes `bytes` at `offset` of province `id`.
    pub fn set(&self, id: u32, offset: usize, bytes: &[u8]) -> &Self {
        self.array.set(id as usize * crate::objects::campaign::province::SIZE + offset, bytes);
        self
    }

    pub fn u8(&self, id: u32, offset: usize, value: u8) -> &Self {
        self.set(id, offset, &[value])
    }

    pub fn u64(&self, id: u32, offset: usize, value: u64) -> &Self {
        self.set(id, offset, &value.to_le_bytes())
    }

    pub fn i32(&self, id: u32, offset: usize, value: i32) -> &Self {
        self.set(id, offset, &value.to_le_bytes())
    }
}

impl Drop for Provinces {
    fn drop(&mut self) {
        super::with_state(|s| s.geography.clear());
        HOLDING.with(|h| h.set(false));
    }
}

/// The game's conditions, for a capability that lists those that do not
/// hold: their scope made and destroyed, and `lines` writing them as the
/// game's tooltip does.
pub fn conditions(image: &Image, lines: *const ()) {
    use super::conditions::{LINES, SCOPE, SCOPE_DESTROY};
    image.function(SCOPE.rva(), no as *const ()).function(SCOPE_DESTROY.rva(), no as *const ()).function(LINES.rva(), lines);
}

/// A country's pools of envoys ([`super::envoys`]), for its field
/// `envoys::POOLS`: of each kind, its envoys ([`envoy`]).
pub fn envoys(kinds: Vec<(super::envoys::Kind, Vec<Object>)>) -> Object {
    use super::envoys::ENVOYS;
    let mut pools = Object::new(0x18);
    for (kind, envoys) in kinds {
        let mut pool = Object::new(ENVOYS + 0x10).vector(ENVOYS, &envoys.iter().map(|e| e.at() as u64).collect::<Vec<_>>());
        pool.kept.extend(envoys);
        pools = pools.points_to(kind as usize, pool);
    }
    pools
}

/// An envoy, away or free, `size` bytes or as far as whether it is away.
pub fn envoy(away: bool, size: usize) -> Object {
    use super::envoys::AWAY;
    Object::new(size.max(AWAY + 8)).u8(AWAY, away.into())
}

/// An army (`land`) or a fleet of `owner`'s with the ID `id`, as
/// [`units`] lets the game find it, `size` bytes or as far as its owner.
pub fn unit(m: &Memory, id: u64, owner: Tag, land: bool, size: usize) -> Object {
    use crate::objects::campaign::{classes, unit};
    let class = if land { classes::ARMY } else { classes::FLEET };
    Object::new(size.max(unit::OWNER + 8)).u64(0, m.at(class) as u64).u64(unit::ID, id).u64(unit::OWNER, owner.0)
}

thread_local! {
    /// The units the game's lookup by ID finds, on this test's thread.
    static UNITS: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The game's lookup of an object by its ID: a unit of [`units`], by where
/// it holds its ID.
extern "C" fn find_id(key: *const u64) -> usize {
    use crate::objects::campaign::unit;
    let id = unsafe { *key };
    UNITS.with(|u| u.borrow().iter().find(|&&at| unsafe { *((at + unit::ID) as *const u64) } == id).map_or(0, |at| at + unit::ID_HOLDER))
}

/// Has the game find `units` by their IDs, and no others.
pub fn units(image: &Image, units: &[&Object]) {
    UNITS.with(|u| *u.borrow_mut() = units.iter().map(|o| o.at()).collect());
    image.function(super::army::FIND_ID.rva(), find_id as *const ());
}

/// The game's localisation, which a test's texts stand for by their keys:
/// a key's text is its first [`game_string::INLINE`] bytes.
pub fn localisation(image: &Image) {
    image.function(game_string::LOCALIZE_AT.rva(), localize_key as *const ());
}

extern "C" fn localize_key(out: usize, key: *const std::ffi::c_char) -> usize {
    let key = unsafe { std::ffi::CStr::from_ptr(key) }.to_bytes();
    let empty = game_string::empty();
    unsafe { std::ptr::copy_nonoverlapping(empty.as_ptr() as *const u8, out as *mut u8, game_string::OBJECT) };
    write_inline(out, &key[..key.len().min(game_string::INLINE)]);
    0
}

/// Writes `text`, at most [`game_string::INLINE`] bytes, into the empty
/// `std::string` at `string`, as a game function writes its reasons.
pub fn write_inline(string: usize, text: impl AsRef<[u8]>) {
    let text = text.as_ref();
    assert!(text.len() <= game_string::INLINE, "{text:?} is longer than a string holds in itself");
    unsafe {
        std::ptr::copy_nonoverlapping(text.as_ptr(), string as *mut u8, text.len());
        *((string + game_string::SIZE) as *mut u64) = text.len() as u64;
    }
}

// Batch one's shapes: a world's provinces, and the game's functions a
// panel's check calls, answering as the test says.

impl Object {
    /// One of the game's arrays at `offset` (`containers::array`), holding
    /// `items`.
    pub fn game_array(mut self, offset: usize, items: &[u64]) -> Self {
        let items: Box<[u64]> = items.into();
        self = self.u64(offset + crate::containers::ARRAY_DATA, items.as_ptr() as u64).i32(offset + crate::containers::ARRAY_COUNT, items.len() as i32);
        self.vectors.push(items);
        self
    }
}

/// Writes `text` into the `std::string` at `string` when there is one, as
/// the game's checks write their reasons and its lookup a localisation's
/// text: held in the string itself when it fits, otherwise in memory of its
/// own, which the test's destructor ([`destroy_string`]) leaves.
///
/// # Safety
/// `string` is null or an empty `std::string` of the framework's, such as
/// the one [`game_string::written`] gives.
pub unsafe fn write_reason(string: usize, text: &[u8]) {
    if string == 0 {
        return;
    }
    unsafe {
        if text.len() <= game_string::INLINE {
            std::ptr::copy_nonoverlapping(text.as_ptr(), string as *mut u8, text.len());
        } else {
            *(string as *mut usize) = Box::leak(text.to_vec().into_boxed_slice()).as_ptr() as usize;
            *((string + game_string::CAPACITY) as *mut usize) = text.len();
        }
        *((string + game_string::SIZE) as *mut usize) = text.len();
    }
}

/// A `std::string`'s destructor, for one whose text is held in itself.
pub extern "C" fn destroy_string(_: usize) -> usize {
    0
}

/// A game function that answers 1: yes, or true.
pub extern "C" fn yes() -> usize {
    1
}

/// A game function that answers 0: no, false, or nothing.
pub extern "C" fn no() -> usize {
    0
}

/// The text of the `std::string` at `string`.
fn text_of(string: usize) -> Vec<u8> {
    game_string::read(&Memory { base: 0 }, string).expect("a test's string")
}

/// The game's `append(this, bytes, count)`, for a test's strings.
extern "C" fn append(string: usize, bytes: *const u8, count: usize) -> usize {
    let mut text = text_of(string);
    text.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, count) });
    unsafe { write_reason(string, &text) };
    0
}

/// A value of the localisation lookup's, made: its text an empty string.
extern "C" fn new_value(value: usize) -> usize {
    let empty = game_string::empty();
    unsafe { std::ptr::copy_nonoverlapping(empty.as_ptr(), (value + game_string::VALUE_TEXT) as *mut usize, empty.len()) };
    0
}

/// The localisation lookup `(out, key, values, count)`, for a test: the
/// key, then each value as ` NAME=text`, so that a test sees which text the
/// framework asked for and with what.
extern "C" fn localize(out: usize, key: *const std::ffi::c_char, values: usize, count: usize) -> usize {
    let mut text = unsafe { std::ffi::CStr::from_ptr(key) }.to_bytes().to_vec();
    for i in 0..count {
        let value = values + i * game_string::VALUE;
        let name = unsafe { std::ffi::CStr::from_ptr(*(value as *const *const std::ffi::c_char)) };
        text.extend_from_slice(format!(" {}=", name.to_string_lossy()).as_bytes());
        text.extend(text_of(value + game_string::VALUE_TEXT));
    }
    unsafe { write_reason(out, &text) };
    0
}

/// A content trigger's verdict `(trigger, scope)`, for a test: the byte at
/// the trigger, which the test sets to 1 for one that holds.
extern "C" fn verdict(trigger: *const u8, _: usize) -> usize {
    unsafe { *trigger as usize }
}

/// A content trigger's lines that do not hold `(trigger, out, ..)`, for a
/// test: none for one that holds, and one line, `unmet`, for one that does
/// not.
extern "C" fn unmet(trigger: *const u8, out: usize) -> usize {
    if unsafe { *trigger } == 0 {
        unsafe { write_reason(out, b"unmet") };
    }
    0
}

impl Image {
    /// The game's conditions, for a test: a trigger holds when its first
    /// byte is 1, and one that does not says `unmet`; with
    /// [`Image::strings`].
    pub fn conditions(&self) -> &Self {
        use super::conditions::{EVALUATE, LINES, SCOPE, SCOPE_DESTROY};
        self.strings()
            .function(SCOPE.rva(), no as *const ())
            .function(SCOPE_DESTROY.rva(), no as *const ())
            .function(EVALUATE.rva(), verdict as *const ())
            .function(LINES.rva(), unmet as *const ())
    }

    /// The game's tokens, the names `names` numbered from 0, as a command's
    /// type and a culture's key are named by theirs; the names kept in what
    /// it answers.
    pub fn tokens(&self, names: &[&str]) -> Object {
        use super::commands::{TOKENS_LAST, TOKENS_READY, TOKEN_NAMES};
        let table = names.iter().enumerate().fold(Object::new(names.len() * game_string::OBJECT), |t, (i, name)| t.string(i * game_string::OBJECT, name));
        self.u64(TOKENS_READY, 1).i32(TOKENS_LAST, names.len() as i32 - 1).u64(TOKEN_NAMES, table.at() as u64);
        table
    }

    /// The game's function a check that writes its reasons has called: the
    /// destructor of the `std::string` it wrote them to.
    pub fn strings(&self) -> &Self {
        self.function(game_string::DESTROY_AT.rva(), destroy_string as *const ())
    }

    /// [`Image::strings`], and the game's localisation lookup as
    /// [`localize`] answers it, with the values it takes; the localisation's
    /// table has no text, so that a name is its key's text.
    pub fn localisation(&self) -> &Self {
        game_string::APPEND.slot().store(append as *const () as usize, std::sync::atomic::Ordering::Release);
        self.u64(game_string::LOCALISATION, (self.base + game_string::LOCALISATION + 8) as u64)
            .u64(game_string::LOCALISATION + 8, 0)
            .function(game_string::FIND_TEXT.rva(), no as *const ());
        self.strings()
            .function(game_string::LOCALIZE_AT.rva(), localize as *const ())
            .function(game_string::VALUE_NEW.rva(), new_value as *const ())
            .function(game_string::VALUE_DESTROY.rva(), destroy_string as *const ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::{byz, tur};

    extern "C" fn twice(value: usize) -> usize {
        value * 2
    }

    #[test]
    fn an_image_s_globals_and_functions_are_at_their_rvas() {
        let image = Image::new();
        let m = image.memory();
        // A global and a function of the game's, at RVAs points check.
        image.i32(crate::objects::campaign::globals::SPECIAL_COUNTRIES[0], 7).function(super::super::IS_PAUSED.rva(), twice as *const ());
        assert_eq!(m.i32(m.at(crate::objects::campaign::globals::SPECIAL_COUNTRIES[0])).unwrap(), 7);
        assert_eq!(super::super::IS_PAUSED.call(m, [21]).unwrap(), 42);
    }

    #[test]
    fn countries_are_listed_at_their_tags_indices_and_found_by_them() {
        let (player, other) = (Object::country(tur(), 0), Object::country(byz(), 0));
        let list = Object::countries(&[&player, &other]);
        let m = Image::new().memory();
        let session = session(m, 0, tur(), player.at(), list.at(), Caller::Remote);
        assert_eq!(super::super::country(m, &session.ctx, byz()).unwrap(), other.at());
        assert_eq!(super::super::country(m, &session.ctx, tur()).unwrap(), player.at());
        assert!(super::super::country(m, &session.ctx, crate::catalogue::fake::wal()).is_err());
    }

    #[test]
    fn an_object_keeps_what_it_points_to_and_its_vectors() {
        let object = Object::new(0x40).points_to(0, Object::new(8).u64(0, 5)).vector(0x10, &[1, 2, 3]).string(0x20, "march");
        let m = Memory { base: 0 };
        assert_eq!(m.u64(m.ptr(object.at()).unwrap()).unwrap(), 5);
        let (begin, end) = (m.ptr(object.at() + 0x10).unwrap(), m.ptr(object.at() + 0x18).unwrap());
        assert_eq!((end - begin, m.u64(begin + 16).unwrap()), (24, 3));
        assert_eq!(game_string::read(&m, object.at() + 0x20).unwrap(), b"march");
    }
}
