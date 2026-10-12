#define _GNU_SOURCE
#include <wayland-client.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <fcntl.h>
#include "xdg-shell-client-protocol.h"
#define MIME "text/plain;charset=utf-8"
#define PAYLOAD "G07B02I-WAYLAND-DND-20261012-8D8F63A1"
static struct wl_display *d;
static struct wl_compositor *comp;
static struct wl_shm *shm;
static struct wl_seat *seat;
static struct wl_pointer *ptr;
static struct wl_data_device_manager *manager;
static struct wl_data_device *device;
static struct wl_data_source *source;
static struct wl_data_offer *offer;
static struct xdg_wm_base *wm;
static struct wl_surface *surface;
static struct xdg_surface *xs;
static struct xdg_toplevel *top;
static struct wl_buffer *buffer;
static int role_source,configured,presses,offers,enter_count,dropped,sent,finished,received;
static char selected_mime[128];
static void source_target(void *v,struct wl_data_source *s,const char *mime){
 (void)v;(void)s;printf("DND_SOURCE_TARGET %s\n",mime?mime:"none");fflush(stdout);
}
static void source_send(void *v,struct wl_data_source *s,const char *mime,int fd){
 (void)v;(void)s;
 if (!strcmp(mime,MIME)){ ssize_t n=write(fd,PAYLOAD,strlen(PAYLOAD));sent=n==(ssize_t)strlen(PAYLOAD);
 printf("DND_SOURCE_SEND bytes=%zd valid=%d\n",n,sent);fflush(stdout);}
 close(fd);
}
static void source_cancel(void *v,struct wl_data_source *s){
 (void)v;(void)s;puts("DND_SOURCE_CANCELLED");fflush(stdout);
}
static void source_action(void *v,struct wl_data_source *s,uint32_t act){
 (void)v;(void)s;printf("DND_SOURCE_ACTION %u\n",act);fflush(stdout);
}
static void source_finished(void *v,struct wl_data_source *s){
 (void)v;(void)s;finished=1;puts("DND_SOURCE_FINISHED");fflush(stdout);
}
static void source_dropped(void*v,struct wl_data_source*s){(void)v;(void)s;puts("DND_SOURCE_DROP_PERFORMED");fflush(stdout);}
static const struct wl_data_source_listener sl={
 .target=source_target,.send=source_send,.cancelled=source_cancel,
 .dnd_drop_performed=source_dropped,.dnd_finished=source_finished,.action=source_action};
static void offered(void *v,struct wl_data_offer *o,const char *mime){
 (void)v;(void)o;
 if (!strcmp(mime,MIME)){strcpy(selected_mime,MIME);offers++;
 puts("DND_TARGET_MIME_ACCEPTABLE");fflush(stdout);}
}
static void offered_source_actions(void*v,struct wl_data_offer*o,uint32_t a){
 (void)v;(void)o;printf("DND_OFFER_SOURCE_ACTIONS %u\n",a);fflush(stdout);
}
static void offered_action(void*v,struct wl_data_offer*o,uint32_t a){
 (void)v;(void)o;printf("DND_OFFER_ACTION %u\n",a);fflush(stdout);
}
static const struct wl_data_offer_listener ol={
 .offer=offered,.source_actions=offered_source_actions,.action=offered_action};
static void data_offer(void*v,struct wl_data_device *dev,struct wl_data_offer *o){
 (void)v;(void)dev;offer=o;wl_data_offer_add_listener(o,&ol,NULL);
 puts("DND_DATA_OFFER");fflush(stdout);
}
static void data_enter(void*v,struct wl_data_device *dev,uint32_t serial,
 struct wl_surface *s,wl_fixed_t x,wl_fixed_t y,struct wl_data_offer *o){
 (void)v;(void)dev;(void)x;(void)y;
 if(s!=surface || !o){puts("DND_INVALID_ENTER");return;}
 enter_count++;offer=o;
 wl_data_offer_accept(o,serial,MIME);
 wl_data_offer_set_actions(o,WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY,
                            WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
 puts("DND_TARGET_ENTER");fflush(stdout);
}
static void data_leave(void*v,struct wl_data_device *dev){
 (void)v;(void)dev;puts("DND_TARGET_LEAVE");fflush(stdout);
}
static void data_motion(void*v,struct wl_data_device *dev,uint32_t tm,
 wl_fixed_t x,wl_fixed_t y){(void)v;(void)dev;(void)tm;(void)x;(void)y;}
static void data_drop(void*v,struct wl_data_device *dev){
 (void)v;(void)dev;dropped=1;
 puts("DND_TARGET_DROP");fflush(stdout);
 if(!offer || strcmp(selected_mime,MIME))return;
 int fd[2];if(pipe(fd))return;
 wl_data_offer_receive(offer,MIME,fd[1]);close(fd[1]);wl_display_flush(d);
 char msg[256]={0};int n=read(fd[0],msg,sizeof(msg)-1);close(fd[0]);
 received=n==(int)strlen(PAYLOAD)&&!memcmp(msg,PAYLOAD,strlen(PAYLOAD));
 printf("DND_TARGET_RECEIVED bytes=%d valid=%d content=%s\n",n,received,msg);
 if(received)wl_data_offer_finish(offer);
 fflush(stdout);
}
static void data_selection(void*v,struct wl_data_device *dev,struct wl_data_offer *o){
 (void)v;(void)dev;(void)o;
}
static const struct wl_data_device_listener dl={
 .data_offer=data_offer,.enter=data_enter,.leave=data_leave,
 .motion=data_motion,.drop=data_drop,.selection=data_selection};
static void pointer_enter(void*v,struct wl_pointer*p,uint32_t serial,
 struct wl_surface*s,wl_fixed_t x,wl_fixed_t y){
 (void)v;(void)p;(void)serial;(void)x;(void)y;
 if(s==surface){puts("DND_POINTER_ENTER");fflush(stdout);}
}
static void pointer_leave(void*v,struct wl_pointer*p,uint32_t serial,struct wl_surface*s){
 (void)v;(void)p;(void)serial;(void)s;
}
static void pointer_motion(void*v,struct wl_pointer*p,uint32_t t,wl_fixed_t x,wl_fixed_t y){
 (void)v;(void)p;(void)t;(void)x;(void)y;}
static void pointer_button(void*v,struct wl_pointer*p,uint32_t serial,
 uint32_t t,uint32_t button,uint32_t state){
 (void)v;(void)p;(void)t;
 if(role_source&&button==272&&state==1 && !presses++){
  source=wl_data_device_manager_create_data_source(manager);
  wl_data_source_add_listener(source,&sl,NULL);
  wl_data_source_offer(source,MIME);
  wl_data_source_set_actions(source,WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY);
  wl_data_device_start_drag(device,source,surface,NULL,serial);
  wl_display_flush(d);
  puts("DND_SOURCE_DRAG_STARTED");fflush(stdout);
 }
}
static void pointer_axis(void*v,struct wl_pointer*p,uint32_t t,uint32_t a,wl_fixed_t val){
 (void)v;(void)p;(void)t;(void)a;(void)val;}
static void pointer_frame(void*v,struct wl_pointer*p){(void)v;(void)p;}
static void pointer_axis_source(void*v,struct wl_pointer*p,uint32_t a){(void)v;(void)p;(void)a;}
static void pointer_axis_stop(void*v,struct wl_pointer*p,uint32_t t,uint32_t a){
 (void)v;(void)p;(void)t;(void)a;}
static void pointer_axis_discrete(void*v,struct wl_pointer*p,uint32_t a,int32_t n){
 (void)v;(void)p;(void)a;(void)n;}
static const struct wl_pointer_listener pl={
 .enter=pointer_enter,.leave=pointer_leave,.motion=pointer_motion,
 .button=pointer_button,.axis=pointer_axis,.frame=pointer_frame,
 .axis_source=pointer_axis_source,.axis_stop=pointer_axis_stop,.axis_discrete=pointer_axis_discrete};
static void seat_caps(void*v,struct wl_seat*s,uint32_t caps){
 (void)v;
 if((caps&WL_SEAT_CAPABILITY_POINTER)&&!ptr){
  ptr=wl_seat_get_pointer(s);wl_pointer_add_listener(ptr,&pl,NULL);
 }}
static void seat_name(void*v,struct wl_seat*s,const char*n){(void)v;(void)s;(void)n;}
static const struct wl_seat_listener seatl={.capabilities=seat_caps,.name=seat_name};
static void ping(void*v,struct xdg_wm_base*w,uint32_t sn){
 (void)v;xdg_wm_base_pong(w,sn);}
static const struct xdg_wm_base_listener wml={.ping=ping};
static void top_config(void*v,struct xdg_toplevel*t,int32_t w,int32_t h,struct wl_array *s){
 (void)v;(void)t;(void)w;(void)h;(void)s;}
static void top_close(void*v,struct xdg_toplevel*t){(void)v;(void)t;exit(3);}
static const struct xdg_toplevel_listener tl={.configure=top_config,.close=top_close};
static void xdg_config(void*v,struct xdg_surface*x,uint32_t sn){
 (void)v;xdg_surface_ack_configure(x,sn);
 if(!configured++){wl_surface_attach(surface,buffer,0,0);
  wl_surface_damage_buffer(surface,0,0,320,240);wl_surface_commit(surface);
  puts("DND_MAPPED");fflush(stdout);}}
static const struct xdg_surface_listener xl={.configure=xdg_config};
static void registry(void*v,struct wl_registry*r,uint32_t id,const char*n,uint32_t ver){
 (void)v;
 if(!strcmp(n,"wl_compositor"))comp=wl_registry_bind(r,id,&wl_compositor_interface,4);
 else if(!strcmp(n,"wl_shm"))shm=wl_registry_bind(r,id,&wl_shm_interface,1);
 else if(!strcmp(n,"wl_seat")){seat=wl_registry_bind(r,id,&wl_seat_interface,5);
  wl_seat_add_listener(seat,&seatl,NULL);}
 else if(!strcmp(n,"xdg_wm_base"))wm=wl_registry_bind(r,id,&xdg_wm_base_interface,1);
 else if(!strcmp(n,"wl_data_device_manager"))
  manager=wl_registry_bind(r,id,&wl_data_device_manager_interface,ver<3?ver:3);
}
static void removed(void*v,struct wl_registry*r,uint32_t id){
 (void)v;(void)r;(void)id;}
static const struct wl_registry_listener rl={.global=registry,.global_remove=removed};
static struct wl_buffer *newbuf(void){
 int len=320*240*4,fd=memfd_create("g07b02i-dnd",MFD_CLOEXEC);
 if(fd<0||ftruncate(fd,len))exit(4);
 uint32_t *mem=mmap(NULL,len,PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);
 if(mem==MAP_FAILED)exit(4);
 for(int i=0;i<320*240;i++)mem[i]=role_source?0x00885022:0x002266BB;
 struct wl_shm_pool *pool=wl_shm_create_pool(shm,fd,len);
 struct wl_buffer *b=wl_shm_pool_create_buffer(pool,0,320,240,320*4,
                                  WL_SHM_FORMAT_XRGB8888);
 wl_shm_pool_destroy(pool);munmap(mem,len);close(fd);return b;
}
int main(int argc,char**argv){
 if(argc!=2|| (strcmp(argv[1],"source")&&strcmp(argv[1],"target")))return 2;
 role_source=!strcmp(argv[1],"source");
 d=wl_display_connect(NULL);if(!d){perror("wl_display_connect");return 3;}
 struct wl_registry*r=wl_display_get_registry(d);wl_registry_add_listener(r,&rl,NULL);
 wl_display_roundtrip(d);wl_display_roundtrip(d);
 if(!comp||!shm||!seat||!ptr||!wm||!manager){puts("DND_GLOBALS_MISSING");return 4;}
 xdg_wm_base_add_listener(wm,&wml,NULL);
 device=wl_data_device_manager_get_data_device(manager,seat);
 wl_data_device_add_listener(device,&dl,NULL);
 surface=wl_compositor_create_surface(comp);buffer=newbuf();
 xs=xdg_wm_base_get_xdg_surface(wm,surface);xdg_surface_add_listener(xs,&xl,NULL);
 top=xdg_surface_get_toplevel(xs);xdg_toplevel_add_listener(top,&tl,NULL);
 xdg_toplevel_set_title(top,role_source?"G07B02I DnD Source":"G07B02I DnD Target");
 wl_surface_commit(surface);
 printf("DND_READY %s\n",argv[1]);fflush(stdout);
 while(wl_display_dispatch(d)>=0){
  if(role_source&&finished)break;
  if(!role_source&&received){wl_display_roundtrip(d);usleep(450000);break;}
 }
 printf("DND_DONE source=%d sent=%d finished=%d target_received=%d dropped=%d\n",
     role_source,sent,finished,received,dropped);fflush(stdout);
 return role_source?(sent&&finished?0:6):(received?0:7);
}
