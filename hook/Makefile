CC      ?= gcc
CFLAGS  ?= -Wall -Wextra -O2 -fPIC
LDFLAGS  = -shared -ldl -lpthread

SOURCES  = tt_fps.c hook_vulkan.c hook_glx.c hook_egl.c
OBJECTS  = $(SOURCES:.c=.o)
TARGET   = libttfps.so

LIBDIR  ?= /usr/local/lib

# Vulkan headers
VK_CFLAGS = $(shell pkg-config --cflags vulkan 2>/dev/null)

.PHONY: all clean install uninstall

all: $(TARGET) VkLayer_tt_fps.json

$(TARGET): $(OBJECTS)
	$(CC) -shared -o $@ $^ $(LDFLAGS)

%.o: %.c tt_fps.h
	$(CC) $(CFLAGS) $(VK_CFLAGS) -c -o $@ $<

# Generate the JSON manifest with the absolute library path
VkLayer_tt_fps.json: VkLayer_tt_fps.json.in
	sed 's|@LIBDIR@|$(LIBDIR)|g' $< > $@

install: $(TARGET) VkLayer_tt_fps.json
	install -d $(DESTDIR)$(LIBDIR)
	install -m 755 $(TARGET) $(DESTDIR)$(LIBDIR)/
	install -d $(DESTDIR)/usr/share/vulkan/implicit_layer.d
	install -m 644 VkLayer_tt_fps.json $(DESTDIR)/usr/share/vulkan/implicit_layer.d/
	install -d $(DESTDIR)/usr/local/bin
	install -m 755 tt-fps $(DESTDIR)/usr/local/bin/

uninstall:
	rm -f $(DESTDIR)$(LIBDIR)/$(TARGET)
	rm -f $(DESTDIR)/usr/share/vulkan/implicit_layer.d/VkLayer_tt_fps.json
	rm -f $(DESTDIR)/usr/local/bin/tt-fps

clean:
	rm -f $(OBJECTS) $(TARGET) VkLayer_tt_fps.json
