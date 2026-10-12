// G07B-02G2: native Wayland protocol capability/activation probe; nested compositor only.
#define _GNU_SOURCE
#include <wayland-client.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include "xdg-shell-client-protocol.h"
#include "pointer-constraints-client-protocol.h"
static struct wl_display *display;
static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct wl_seat *seat;
static struct wl_pointer *pointer;
static struct wl_surface *surface;
static struct wl_buffer *buffer;
static struct xdg_wm_base *wm;
static struct xdg_surface *xdgs;
static struct xdg_toplevel *top;
static struct zwp_pointer_constraints_v1 *constraints;
static struct zwp_locked_pointer_v1 *locked;
static struct zwp_confined_pointer_v1 *confined;
static const char *mode;
static int configured, entered, activated, failures;
static void on_locked(void *d, struct zwp_locked_pointer_v1 *p) {
  (void)d;(void)p;activated=1; puts("G2_CONSTRAINT_ACTIVE locked");fflush(stdout);
}
static void on_unlocked(void *d, struct zwp_locked_pointer_v1 *p) {
  (void)d;(void)p;puts("G2_CONSTRAINT_DEACTIVATED locked");fflush(stdout);
}
static const struct zwp_locked_pointer_v1_listener locked_listener={
  .locked=on_locked,.unlocked=on_unlocked
};
static void on_confined(void *d, struct zwp_confined_pointer_v1 *p) {
  (void)d;(void)p;activated=1;puts("G2_CONSTRAINT_ACTIVE confined");fflush(stdout);
}
static void on_unconfined(void *d, struct zwp_confined_pointer_v1 *p) {
  (void)d;(void)p;puts("G2_CONSTRAINT_DEACTIVATED confined");fflush(stdout);
}
static const struct zwp_confined_pointer_v1_listener confined_listener={
  .confined=on_confined,.unconfined=on_unconfined
};
static void pointer_enter(void *d, struct wl_pointer *p, uint32_t serial,
                          struct wl_surface *s, wl_fixed_t x, wl_fixed_t y) {
  (void)d;(void)p;(void)serial;(void)x;(void)y;
  if (s!=surface || entered++) return;
  puts("G2_POINTER_ENTER");fflush(stdout);
  if (!constraints) {puts("G2_CONSTRAINT_GLOBAL_MISSING");failures++;return;}
  if (!strcmp(mode,"lock")) {
    locked=zwp_pointer_constraints_v1_lock_pointer(constraints,surface,pointer,NULL,
       ZWP_POINTER_CONSTRAINTS_V1_LIFETIME_ONESHOT);
    zwp_locked_pointer_v1_add_listener(locked,&locked_listener,NULL);
  } else {
    confined=zwp_pointer_constraints_v1_confine_pointer(constraints,surface,pointer,NULL,
       ZWP_POINTER_CONSTRAINTS_V1_LIFETIME_ONESHOT);
    zwp_confined_pointer_v1_add_listener(confined,&confined_listener,NULL);
  }
  wl_surface_commit(surface);
  wl_display_flush(display);
}
static void pointer_leave(void *d,struct wl_pointer *p,uint32_t serial,struct wl_surface *s){
  (void)d;(void)p;(void)serial;(void)s;puts("G2_POINTER_LEAVE");fflush(stdout);
}
static void pointer_motion(void*d,struct wl_pointer*p,uint32_t t,wl_fixed_t x,wl_fixed_t y){
 (void)d;(void)p;(void)t;
 static unsigned events=0;
 if (++events<=12) printf("H_POINTER_MOTION x=%.3f y=%.3f\n",wl_fixed_to_double(x),wl_fixed_to_double(y));
 fflush(stdout);
}
static void pointer_button(void*d,struct wl_pointer*p,uint32_t s,uint32_t t,uint32_t b,uint32_t state){
 (void)d;(void)p;(void)s;(void)t;(void)b;(void)state;
}
static void pointer_axis(void*d,struct wl_pointer*p,uint32_t t,uint32_t axis,wl_fixed_t v){
 (void)d;(void)p;(void)t;(void)axis;(void)v;
}
static void pointer_frame(void*d,struct wl_pointer*p){(void)d;(void)p;}
static void pointer_axis_source(void*d,struct wl_pointer*p,uint32_t source){
 (void)d;(void)p;(void)source;
}
static void pointer_axis_stop(void*d,struct wl_pointer*p,uint32_t t,uint32_t axis){
 (void)d;(void)p;(void)t;(void)axis;
}
static void pointer_axis_discrete(void*d,struct wl_pointer*p,uint32_t a,int32_t v){
 (void)d;(void)p;(void)a;(void)v;
}
static const struct wl_pointer_listener pointer_listener={
 .enter=pointer_enter,.leave=pointer_leave,.motion=pointer_motion,
 .button=pointer_button,.axis=pointer_axis,.frame=pointer_frame,
 .axis_source=pointer_axis_source,.axis_stop=pointer_axis_stop,.axis_discrete=pointer_axis_discrete
};
static void seat_caps(void *d,struct wl_seat *s,uint32_t caps){
  (void)d;if ((caps&WL_SEAT_CAPABILITY_POINTER) && !pointer) {
    pointer=wl_seat_get_pointer(s);
    wl_pointer_add_listener(pointer,&pointer_listener,NULL);
  }
}
static void seat_name(void *d,struct wl_seat *s,const char *name){
  (void)d;(void)s;(void)name;
}
static const struct wl_seat_listener seat_listener={.capabilities=seat_caps,.name=seat_name};
static void registry_add(void *d,struct wl_registry *r,uint32_t id,
                         const char *iface,uint32_t ver) {
  (void)d;(void)ver;
  if (!strcmp(iface,"wl_compositor"))
    compositor=wl_registry_bind(r,id,&wl_compositor_interface,4);
  else if (!strcmp(iface,"wl_shm"))
    shm=wl_registry_bind(r,id,&wl_shm_interface,1);
  else if (!strcmp(iface,"wl_seat")) {
    seat=wl_registry_bind(r,id,&wl_seat_interface,5);
    wl_seat_add_listener(seat,&seat_listener,NULL);
  } else if (!strcmp(iface,"xdg_wm_base"))
    wm=wl_registry_bind(r,id,&xdg_wm_base_interface,1);
  else if (!strcmp(iface,"zwp_pointer_constraints_v1"))
    constraints=wl_registry_bind(r,id,&zwp_pointer_constraints_v1_interface,1);
}
static void registry_remove(void*d,struct wl_registry*r,uint32_t id){
  (void)d;(void)r;(void)id;
}
static const struct wl_registry_listener registry_listener={
  .global=registry_add,.global_remove=registry_remove
};
static void wm_ping(void *d,struct xdg_wm_base*w,uint32_t serial){
  (void)d;xdg_wm_base_pong(w,serial);
}
static const struct xdg_wm_base_listener wm_listener={.ping=wm_ping};
static void toplevel_configure(void*d,struct xdg_toplevel*t,
                               int32_t w,int32_t h,struct wl_array *states){
  (void)d;(void)t;(void)w;(void)h;(void)states;
}
static void toplevel_close(void*d,struct xdg_toplevel*t){
  (void)d;(void)t;failures++;puts("G2_WINDOW_CLOSED");fflush(stdout);
}
static const struct xdg_toplevel_listener toplevel_listener={
  .configure=toplevel_configure,.close=toplevel_close
};
static void xdg_configure(void*d,struct xdg_surface*x,uint32_t serial){
  (void)d;xdg_surface_ack_configure(x,serial);
  if (!configured++) {
    wl_surface_attach(surface,buffer,0,0);
    wl_surface_damage_buffer(surface,0,0,320,240);
    wl_surface_commit(surface);
    puts("G2_SURFACE_MAPPED");fflush(stdout);
  }
}
static const struct xdg_surface_listener xdg_listener={.configure=xdg_configure};
static int create_buffer(void){
  const int width=320,height=240,bytes=width*height*4;
  int fd=memfd_create("g2-pointer-constraints",MFD_CLOEXEC);
  if (fd<0 || ftruncate(fd,bytes))return -1;
  uint32_t *pixels=mmap(NULL,bytes,PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);
  if (pixels==MAP_FAILED)return -1;
  for(int i=0;i<width*height;i++)pixels[i]=0x00336699;
  struct wl_shm_pool*pool=wl_shm_create_pool(shm,fd,bytes);
  buffer=wl_shm_pool_create_buffer(pool,0,width,height,width*4,WL_SHM_FORMAT_XRGB8888);
  wl_shm_pool_destroy(pool);
  munmap(pixels,bytes);close(fd);return buffer?0:-1;
}
int main(int argc,char **argv){
  mode=argc>1?argv[1]:"confine";
  if (strcmp(mode,"lock") && strcmp(mode,"confine"))return 2;
  display=wl_display_connect(NULL);
  if (!display){puts("G2_CONNECT_FAILED");return 3;}
  struct wl_registry *registry=wl_display_get_registry(display);
  wl_registry_add_listener(registry,&registry_listener,NULL);
  wl_display_roundtrip(display);wl_display_roundtrip(display);
  if (!compositor || !shm || !seat || !wm || !pointer || !constraints){
    puts("G2_GLOBALS_INCOMPLETE");return 4;
  }
  xdg_wm_base_add_listener(wm,&wm_listener,NULL);
  surface=wl_compositor_create_surface(compositor);
  if (create_buffer())return 5;
  xdgs=xdg_wm_base_get_xdg_surface(wm,surface);
  xdg_surface_add_listener(xdgs,&xdg_listener,NULL);
  top=xdg_surface_get_toplevel(xdgs);
  xdg_toplevel_add_listener(top,&toplevel_listener,NULL);
  xdg_toplevel_set_title(top,"G07B02G2 Pointer Constraints Probe");
  wl_surface_commit(surface);
  puts("G2_READY");fflush(stdout);
  for(int i=0;i<5000 && !failures;i++){
    struct pollfd{int fd;short events,revents;};
    // Dispatch callback events without depending on a GLib/GTK event loop.
    wl_display_dispatch(display);
  }
  printf("G2_RESULT %s activated=%d entered=%d configured=%d\n",mode,activated,entered,configured);
  fflush(stdout);wl_display_disconnect(display);
  return activated?0:7;
}
