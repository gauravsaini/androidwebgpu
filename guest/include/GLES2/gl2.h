#ifndef __gl2_h_
#define __gl2_h_

#include <stdint.h>
#include <stddef.h>
#include <GLES2/gl2platform.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void GLvoid;
typedef char GLchar;
typedef unsigned int GLenum;
typedef unsigned char GLboolean;
typedef unsigned int GLbitfield;
typedef int8_t GLbyte;
typedef int16_t GLshort;
typedef int32_t GLint;
typedef int32_t GLsizei;
typedef uint8_t GLubyte;
typedef uint16_t GLushort;
typedef uint32_t GLuint;
typedef float GLfloat;
typedef float GLclampf;
typedef int32_t GLfixed;

#define GL_FALSE 0
#define GL_TRUE 1

#define GL_COLOR_BUFFER_BIT 0x00004000
#define GL_DEPTH_BUFFER_BIT 0x00000100
#define GL_STENCIL_BUFFER_BIT 0x00000400

#define GL_TRIANGLES 0x0004
#define GL_TRIANGLE_STRIP 0x0005
#define GL_TRIANGLE_FAN 0x0006

GL_APICALL void GL_APIENTRY glClearColor(GLclampf red, GLclampf green, GLclampf blue, GLclampf alpha);
GL_APICALL void GL_APIENTRY glClear(GLbitfield mask);
GL_APICALL void GL_APIENTRY glDrawArrays(GLenum mode, GLint first, GLsizei count);
GL_APICALL void GL_APIENTRY glViewport(GLint x, GLint y, GLsizei width, GLsizei height);

#ifdef __cplusplus
}
#endif

#endif
